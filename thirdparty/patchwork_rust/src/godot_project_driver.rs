use std::{
  collections::HashMap, future::Future, str::FromStr, sync::{
      mpsc::{Receiver, Sender},
      Arc, Mutex,
  }
};
use std::collections::HashSet;
use std::env::var;
use ::safer_ffi::prelude::*;

use automerge::{patches::TextRepresentation, transaction::Transactable, Automerge, Change, ChangeHash, ObjType, PatchLog, ReadDoc, TextEncoding, ROOT};
use automerge_repo::{tokio::FsStorage, ConnDirection, DocHandle, DocumentId, Repo, RepoHandle};
use autosurgeon::{bytes, hydrate, reconcile, Hydrate, Reconcile};
use futures::{channel::mpsc::UnboundedSender, executor::block_on, FutureExt, StreamExt};
use std::ffi::c_void;
use std::ops::Deref;
use std::os::raw::c_char;
use crate::godot_project::get_linked_docs_of_branch;

// use godot::prelude::*;
use tokio::{net::TcpStream, runtime::Runtime};

use crate::{doc_handle_map::DocHandleMap, doc_utils::SimpleDocReader, godot_project::Branch};

const SERVER_URL: &str = "104.131.179.247:8080";

enum DriverInputEvent {
  InitBranchesMetadataDoc {
    doc_id: DocumentId,
  }, 


  // only trigger this event internally in the driver
  NewDocHandle {
    doc_handle: DocHandle,
  }
}

enum DriverOutputEvent {
    DocHandleChanged {
        doc_handle: DocHandle,
    },
    BranchesUpdated {
        branches: HashMap<DocumentId, String>,
    },
}

struct Driver {
    runtime: Runtime,
    repo_handle: RepoHandle,
}

struct ProjectState {
    main_branch_doc_id: DocumentId,
    branches: HashMap<DocumentId, Branch>,
    checked_out_doc_id: DocumentId,  
}

impl Driver {
    fn create() -> Self {
        let runtime: Runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let _guard = runtime.enter();

        let storage = FsStorage::open("/tmp/automerge-godot-data").unwrap();
        let repo = Repo::new(None, Box::new(storage));
        let repo_handle = repo.run();

        return Self {
            runtime,
            repo_handle,
        };
    }

    fn spawn(&self, rx: futures::channel::mpsc::UnboundedReceiver<DriverInputEvent>, tx: futures::channel::mpsc::UnboundedSender<DriverOutputEvent>) {
        // Spawn connection task
        self.spawn_connection_task();

        // Spawn sync task for all doc handles
        self.spawn_driver_task(rx, tx);
    }

    fn spawn_connection_task(&self) {
        let repo_handle_clone = self.repo_handle.clone();

        self.runtime.spawn(async move {
            println!("start a client");

            // Start a client.
            let stream = loop {
                // Try to connect to a peer
                let res = TcpStream::connect(SERVER_URL).await;
                if let Err(e) = res {
                    println!("error connecting: {:?}", e);
                    continue;
                }
                break res.unwrap();
            };

            println!("connect repo");

            if let Err(e) = repo_handle_clone
                .connect_tokio_io(SERVER_URL, stream, ConnDirection::Outgoing)
                .await
            {
                println!("Failed to connect: {:?}", e);
                return;
            }

            println!("connected successfully!");
        });
    }

    fn spawn_driver_task(&self, mut rx: futures::channel::mpsc::UnboundedReceiver<DriverInputEvent>, tx: futures::channel::mpsc::UnboundedSender<DriverOutputEvent>) {
        let repo_handle = self.repo_handle.clone();

        self.runtime.spawn(async move {
            // global state
            let mut state = DriverState {
              doc_handles: HashMap::new(),
              repo_handle,
              state: None,
              tx: tx.clone(),
            };

            let mut all_doc_changes =  futures::stream::SelectAll::new();

            // Now, drive the SelectAll and also wait for any new documents to arrive and add
            // them to the selectall
            loop {
                futures::select! {
                    changed_doc_handle = all_doc_changes.select_next_some() => {


                      tx.unbounded_send(DriverOutputEvent::DocHandleChanged { doc_handle: changed_doc_handle }).unwrap();
                    },

                    message = rx.select_next_some() => {
                        match message {
                            DriverInputEvent::InitBranchesMetadataDoc { doc_id } => {
                              let doc_handle = state.request_document(&doc_id).await;

                              // todo: load branches
                            }

                            DriverInputEvent::NewDocHandle { doc_handle } => {
                                let change_stream = handle_changes(doc_handle.clone()).filter_map(move |diff| {

                                  // trigger the load all binary files


                                  let doc_handle = doc_handle.clone();
                                  async move {
                                      if diff.is_empty() {
                                          None
                                      } else {
                                          Some(
                                            doc_handle
                                          )
                                      }
                                  }
                              });                      

                              all_doc_changes.push(change_stream.boxed());
                            }
                        }
                    }
                }
            }
        });  
    }
}


enum DocHandleType {
  BranchDoc,
  BinaryDoc,
  BranchesMetadataDoc,
}

// what should happen if you receive an update doc handle for each type
// BranchDoc -> check if all the binary files are loaded if not don't update the heads so the user sees an old version
// BinaryDoc -> check the checked out branch if this new file is the last missing binary file then update the heads

struct DocHandleWithType {
  doc_handle: DocHandle,
  doc_handle_type: DocHandleType,
  heads: Vec<ChangeHash>,
}

struct DriverState {
    doc_handles: HashMap<DocumentId, DocHandle>,
    repo_handle: RepoHandle,
    state: Option<ProjectState>,
    tx: futures::channel::mpsc::UnboundedSender<DriverOutputEvent>,
}

impl DriverState {
    async fn request_document(&mut self, doc_id: &DocumentId) -> DocHandle {
        let doc_handle = self.repo_handle.request_document(doc_id.clone()).await.unwrap();

        self.add_handle(doc_handle.clone());

        doc_handle
    }

    fn add_handle(&mut self, doc_handle: DocHandle) {
        let doc_id = doc_handle.document_id();
        if self.doc_handles.insert(doc_id.clone(), doc_handle.clone()).is_none() {

            // self.tx.unbounded_send(DriverOutputEvent::DocHandleChanged { doc_handle: doc_handle.clone() }).unwrap();
        }


        self.tx.unbounded_send(DriverOutputEvent::DocHandleChanged { doc_handle: doc_handle.clone() }).unwrap();      
    }
}

fn handle_changes(handle: DocHandle) -> impl futures::Stream<Item = Vec<automerge::Patch>> + Send {
    futures::stream::unfold(handle, |doc_handle| async {
        let heads_before = doc_handle.with_doc(|d| d.get_heads().to_vec());
        let _ = doc_handle.changed().await;
        if crate::godot_project::is_branch_doc(&doc_handle) {}
        // todo: if this is a branch doc, check if all the binary files are loaded if not don't update the heads so the user sees an old version

        let heads_after = doc_handle.with_doc(|d| d.get_heads().to_vec());
        let diff = doc_handle.with_doc(|d| {
            d.diff(
                &heads_before,
                &heads_after,
                TextRepresentation::String(TextEncoding::Utf8CodeUnit),
            )
        });

        Some((diff, doc_handle))
    })
}
