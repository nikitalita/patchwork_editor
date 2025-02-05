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
use futures::{channel::mpsc::{UnboundedReceiver, UnboundedSender}, executor::block_on, FutureExt, StreamExt};
use std::ffi::c_void;
use std::ops::Deref;
use std::os::raw::c_char;
// use godot::prelude::*;
use tokio::{net::TcpStream, runtime::Runtime};

use crate::{doc_handle_map::DocHandleMap, doc_utils::SimpleDocReader, godot_project_driver::{DriverInputEvent, DriverOutputEvent, GodotProjectDriver}};

#[derive(Debug, Clone, Reconcile, Hydrate, PartialEq)]
struct BinaryFile {
    content: Vec<u8>,
}


#[derive(Debug, Clone, Reconcile, Hydrate, PartialEq)]
struct FileEntry {
    content: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Clone, Reconcile, Hydrate, PartialEq)]
struct GodotProjectDoc {
    files: HashMap<String, FileEntry>,
    state: HashMap<String, HashMap<String, String>>,
}

type AutoMergeSignalCallback = extern "C" fn(*mut c_void, *const std::os::raw::c_char, *const *const std::os::raw::c_char, usize) -> ();


#[derive(Debug, Clone, Reconcile, Hydrate, PartialEq)]
pub struct BranchesMetadataDoc {
    pub main_doc_id: String,
    pub branches: HashMap<String, Branch>,
}

#[derive(Debug, Clone, Reconcile, Hydrate, PartialEq)]
pub struct Branch {
    name: String,
    id: String,
    is_merged: bool
}

#[derive(Clone)]
enum SyncEvent {
    NewDoc { doc_id: DocumentId, doc_handle: DocHandle },
    DocChanged { doc_id: DocumentId },
    CheckedOutBranch { doc_id: DocumentId },
}


#[derive(Debug, Clone, Reconcile, Hydrate, PartialEq)]
pub enum StringOrPackedByteArray {
    String(String),
    PackedByteArray(Vec<u8>),
}


#[derive(Debug, Clone)]
struct GodotProjectState {
    checked_out_doc_id: DocumentId,
    branches_metadata_doc_id: DocumentId,
}

// #[derive(GodotClass)]
// #[class(no_init, base=Node)]
pub struct GodotProject_rs {
    branches: HashMap<String, Branch>,
    doc_handles: HashMap<DocumentId, DocHandle>,
    checked_out_branch_doc_id: Option<DocumentId>,
    signal_user_data: *mut c_void,
    signal_callback: AutoMergeSignalCallback,
    driver: GodotProjectDriver,
    driver_input_tx: UnboundedSender<DriverInputEvent>,
    driver_output_rx: UnboundedReceiver<DriverOutputEvent>,
}

const SERVER_URL: &str = "104.131.179.247:8080";
static SIGNAL_BRANCHES_CHANGED: std::sync::LazyLock<std::ffi::CString> = std::sync::LazyLock::new(|| std::ffi::CString::new("branches_changed").unwrap());
static SIGNAL_FILES_CHANGED: std::sync::LazyLock<std::ffi::CString> = std::sync::LazyLock::new(|| std::ffi::CString::new("files_changed").unwrap());
static SIGNAL_CHECKED_OUT_BRANCH: std::sync::LazyLock<std::ffi::CString> = std::sync::LazyLock::new(|| std::ffi::CString::new("checked_out_branch").unwrap());
static SIGNAL_FILE_CHANGED: std::sync::LazyLock<std::ffi::CString> = std::sync::LazyLock::new(|| std::ffi::CString::new("file_changed").unwrap());
static SIGNAL_STARTED: std::sync::LazyLock<std::ffi::CString> = std::sync::LazyLock::new(|| std::ffi::CString::new("started").unwrap());
static SIGNAL_INITIALIZED: std::sync::LazyLock<std::ffi::CString> = std::sync::LazyLock::new(|| std::ffi::CString::new("initialized").unwrap());
// convert a slice of strings to a slice of char * strings (e.g. *const std::os::raw::c_char)
fn to_c_strs(strings: &[&str]) -> Vec<std::ffi::CString> {
    strings.iter().map(|s| std::ffi::CString::new(*s).unwrap()).collect()
}
fn strings_to_c_strs(strings: &[String]) -> Vec<std::ffi::CString> {
    strings.iter().map(|s| std::ffi::CString::new(s.as_str()).unwrap()).collect()
}

// convert a HashMap to a slice of char * strings; e.g. ["key1", "value1", "key2", "value2"]
fn to_c_strs_from_dict(dict: &HashMap<&str, String>) -> Vec<std::ffi::CString> {
    let mut c_strs = Vec::new();
    for (key, value) in dict.iter() {
        c_strs.push(std::ffi::CString::new(*key).unwrap());
        c_strs.push(std::ffi::CString::new(value.as_str()).unwrap());
    }
    c_strs
}

// convert a slice of std::ffi::CString to a slice of *const std::os::raw::c_char
fn to_char_stars(c_strs: &[std::ffi::CString]) -> Vec<*const std::os::raw::c_char> {
    c_strs.iter().map(|s| s.as_ptr()).collect()
}

// #[godot_api]
impl GodotProject_rs {
    // #[signal]
    // fn checked_out_branch(branch_id: String);

    // #[signal]
    // fn files_changed();

    // #[signal]
    // fn branches_changed();

    // #[func]
    // hack: pass in empty string to create a new doc
    // godot rust doens't seem to support Option args
    fn create(maybe_branches_metadata_doc_id: String, signal_user_data: *mut c_void, signal_callback: AutoMergeSignalCallback) -> GodotProject_rs /*-> Gd<Self>*/ {    

        let (driver_input_tx, driver_input_rx) = futures::channel::mpsc::unbounded();
        let (driver_output_tx, driver_output_rx) = futures::channel::mpsc::unbounded();

        let driver = GodotProjectDriver::create();

        driver.spawn(driver_input_rx, driver_output_tx);

        driver_input_tx.unbounded_send(DriverInputEvent::InitBranchesMetadataDoc { doc_id: DocumentId::from_str(&maybe_branches_metadata_doc_id).unwrap() }).unwrap();  

        GodotProject_rs {
            branches: HashMap::new(),
            doc_handles: HashMap::new(),
            checked_out_branch_doc_id: None,
            signal_user_data,
            signal_callback,
            driver,
            driver_input_tx,
            driver_output_rx,
        }    
    }

    // PUBLIC API

    // #[func]
    fn stop(&self) {
      // todo
    }

    fn get_checked_out_branch_handle (&self) -> Option<DocHandle>  {
        match self.checked_out_branch_doc_id.clone() {
            Some(checked_out_branch_doc_id) => {
               self.doc_handles.get(&checked_out_branch_doc_id).cloned()
            }
            None => {
                println!("warning: tried to access checked out doc when no branch was checked out");
                return None
            }
        }
    }

    fn get_checked_out_branch_doc (&self) -> Option<Automerge>  {
        self.get_checked_out_branch_handle().map(|doc_handle| doc_handle.with_doc(|d| d.clone()))
    }

    // #[func]
    fn get_doc_id(&self) -> String {    
        todo!("not implemented");
        // self.branches_metadata_doc_id.to_string()
    }


    fn get_heads(&self) -> Vec<String> /* String[] */ {    
        todo!("not implemented");
        // self.get_checked_out_doc_handle().with_doc(|d| {        
        //     d.get_heads()
        //     .to_vec()
        //     .iter()
        //     .map(|h| h.to_string())
        //     .collect::<Vec<String>>()
        // })
    }

    fn list_all_files(&self) -> Vec<String> {
        let doc = match self.get_checked_out_branch_doc() {
            Some(doc) => doc,
            None => return vec![]
        };

        let files = match doc.get_obj_id(ROOT, "files") {
            Some(files) => files,
            _ => {
                return vec![];
            }
        };
        doc.keys(files).collect::<Vec<String>>()
    }

    fn get_file(&self, path: String) -> Option<StringOrPackedByteArray> {
        todo!("not implemented");
        // let mut content_doc_id_result: Option<DocumentId> = None;
        // let result = self.get_checked_out_doc_handle().with_doc(|doc| {
        //     let files = doc.get(ROOT, "files").unwrap().unwrap().1;
        //     // does the file exist?
        //     let file_entry = match doc.get(files, path) {
        //         Ok(Some((automerge::Value::Object(ObjType::Map), file_entry))) => file_entry,
        //         _ => return None,
        //     };

        //     // try to read file as text
        //     match doc.get(&file_entry, "content") {
        //         Ok(Some((automerge::Value::Object(ObjType::Text), content))) => {
        //             match doc.text(content) {
        //                 Ok(text) => return Some(StringOrPackedByteArray::String(text.to_string())),
        //                 Err(_) => {}
        //             }
        //         },
        //         _ => {}
        //     }

        //     // ... otherwise try to read as linked binary doc


        //     if let Some(url) = doc.get_string(&file_entry, "url") {

        //         // parse doc url
        //         let doc_id = match parse_automerge_url(&url) {
        //             Some(url) => url,
        //             _ => return None
        //         };
        //         content_doc_id_result = Some(doc_id);
        //     };

        //     None
        // });
        // if result.is_none() {
        //     if let Some(content_doc_id) = content_doc_id_result {
        //         // // read content doc
        //         let content_doc = match self.doc_handle_map.get_doc(&content_doc_id) {
        //             Some(doc) => doc,
        //             _ => return None
        //         };

        //         // get content of binary file
        //         if let Some(bytes) = content_doc.get_bytes(ROOT, "content") {
        //             return Some(StringOrPackedByteArray::PackedByteArray(bytes));
        //         };
        //     }
        // }

        // result
    }

    fn get_file_at(&self, path: String, heads: Vec<String> /* String[] */) -> Option<String> /* String? */
    {
        todo!("not implemented");
        // self.get_checked_out_doc_handle().with_doc(|doc| {

        // let heads: Vec<ChangeHash> = heads
        //     .iter()
        //     .map(|h| ChangeHash::from_str(h.to_string().as_str()).unwrap())
        //     .collect();

        // let files = doc.get(ROOT, "files").unwrap().unwrap().1;

        // return match doc.get_at(files, path, &heads) {
        //     Ok(Some((value, _))) => Some(value.into_string().unwrap_or_default()),
        //     _ => None,
        // };  

        // })
    }


    fn get_changes(&self) -> Vec<String> /* String[]  */ {
        todo!("not implemented");
        // self.get_checked_out_doc_handle().with_doc(|doc| {


        // doc.get_changes(&[])
        //     .to_vec()
        //     .iter()
        //     .map(|c| c.hash().to_string())
        //     .collect::<Vec<String>>()
        // })
    }

    // #[func]
    fn save_file(&self, path: String, heads: Option<Vec<ChangeHash>>, content: StringOrPackedByteArray) {
        todo!("not implemented");
        // // ignore if file is already up to date
        // if let Some(stored_content) = self.get_file(path.clone()) {
        //     if stored_content == content {
        //         println!("file {:?} is already up to date", path.clone());
        //         return;
        //     }
        // }

        // self.get_checked_out_doc_handle()
        // .with_doc_mut(|d| {    
        //         let mut tx = match heads {
        //             Some(heads) => {
        //                 d.transaction_at(PatchLog::inactive(TextRepresentation::String(TextEncoding::Utf8CodeUnit)), &heads)
        //             },
        //             None => {
        //                 d.transaction()
        //             }
        //         };

        //         let files = match tx.get(ROOT, "files") {
        //             Ok(Some((automerge::Value::Object(ObjType::Map), files))) => files,
        //             _ => panic!("Invalid project doc, doesn't have files map"),
        //         };

        //         match content {
        //             StringOrPackedByteArray::String(content) => {
        //                 println!("write string {:}", path);

        //                 // get existing file url or create new one                        
        //                 let file_entry = match tx.get(&files, &path) {
        //                     Ok(Some((automerge::Value::Object(ObjType::Map), file_entry))) => file_entry,
        //                     _ => tx.put_object(files, &path, ObjType::Map).unwrap()
        //                 };

        //                 // delete url in file entry if it previously had one
        //                 if let Ok(Some((_, _))) = tx.get(&file_entry, "url") {
        //                     let _ = tx.delete(&file_entry, "url");
        //                 }

        //                 // either get existing text or create new text
        //                 let content_key = match tx.get(&file_entry, "content") {
        //                     Ok(Some((automerge::Value::Object(ObjType::Text), content))) => content,
        //                     _ => tx.put_object(&file_entry, "content", ObjType::Text).unwrap(),
        //                 };
        //                 let _ = tx.update_text(&content_key, &content);
        //             },
        //             StringOrPackedByteArray::PackedByteArray(content) => {
        //                 println!("write binary {:}", path);

        //                 // create content doc
        //                 let content_doc_id = self.create_doc(|d| {
        //                     let mut tx = d.transaction();
        //                     let _ = tx.put(ROOT, "content", content.to_vec());
        //                     tx.commit();
        //                 });

        //                 // write url to content doc into project doc
        //                 let file_entry = tx.put_object(files, path, ObjType::Map);
        //                 let _ = tx.put(file_entry.unwrap(), "url", format!("automerge:{}", content_doc_id));
        //             },
        //         }

        //         tx.commit();
        //     });
    }

    fn merge_branch(&self, branch_id: String) {
        todo!("not implemented");
        // let mut branches_metadata = self.get_branches_metadata_doc();
      
        // // merge branch into main

        // let branch_doc = self.doc_handle_map.get_handle(&DocumentId::from_str(&branch_id).unwrap()).unwrap();
        // let main_doc_id = DocumentId::from_str(branches_metadata.main_doc_id.as_str()).unwrap();

        // branch_doc.with_doc_mut(|branch_doc| {
        //     self.update_doc(&main_doc_id, |d| {
        //         d.merge(branch_doc);
        //     });
        // });
        
        // // mark branch as merged

        // let branch = branches_metadata.branches.get_mut(&branch_id).unwrap();
        // branch.is_merged = true;

        // self.update_doc(&self.branches_metadata_doc_id, |d| {
        //     let mut tx = d.transaction();
        //     reconcile(&mut tx, branches_metadata).unwrap();
        //     tx.commit();
        // });
    }


    fn create_branch(&self, name: String) -> String {
        todo!("not implemented");
        // let mut branches_metadata = self.get_branches_metadata_doc();

        // let main_doc_id = DocumentId::from_str(&branches_metadata.main_doc_id).unwrap();
        // let new_doc_id = self.clone_doc(main_doc_id);

        // branches_metadata.branches.insert(
        //     new_doc_id.to_string(),
        //     Branch {
        //         is_merged: false,
        //         name,
        //         id: new_doc_id.to_string(),
        //     },
        // );

        // self.get_branches_metadata_doc_handle().with_doc_mut(|d| {
        //     let mut tx = d.transaction();
        //     reconcile(&mut tx, branches_metadata).unwrap();
        //     tx.commit();
        // });

        // new_doc_id.to_string()
    }

    // checkout branch in a separate thread
    // ensures that all linked docs are loaded before checking out the branch
    // todo: emit a signal when the branch is checked out
    // 
    // current workaround is to call get_checked_out_branch_id every frame and check if has changed in GDScript

    fn checkout_branch(&mut self, branch_doc_id: String) {
        let branch_doc_id = match DocumentId::from_str(&branch_doc_id) {
            Ok(id) => id,
            Err(e) => {
                println!("invalid branch doc id: {:?}", e);
                return;
            }
        };

        self.driver_input_tx.unbounded_send(DriverInputEvent::CheckoutBranch { branch_doc_id }).unwrap();
    }

    fn get_branches(&self) -> Vec<String> /* { name: String, id: String }[] */ {
        return self.branches.values().flat_map(|branch| {
            vec![
                "name".to_string(),
                branch.name.clone(),
                "id".to_string(),
                branch.id.clone(),
            ]
        }).collect::<Vec<String>>();
    }

    fn get_checked_out_branch_id(&self) -> String {
        todo!("not implemented");
        // return self.checked_out_doc_id.to_string();
    }

    // State api

    fn set_state_int (&self, entity_id: String, prop: String, value: i64) {
        todo!("not implemented");
        // // let checked_out_doc_handle = self.get_checked_out_doc_handle();
        
        // checked_out_doc_handle.with_doc_mut(|d| {
        //     let mut tx = d.transaction();
        //     let state = match tx.get_obj_id(ROOT, "state") {
        //         Some(id) => id,
        //         _ => {
        //             println!("failed to load state");
        //             return 
        //         }
        //     };


        //     match tx.get_obj_id(&state, &entity_id) {
        //         Some(id) => {
        //             let _ = tx.put(id, prop, value);
        //         },                
                
        //         None => {
        //             match tx.put_object(state, &entity_id, ObjType::Map) {
        //                 Ok(id) => {
        //                     let _ = tx.put(id, prop, value);
        //                 },
        //                 Err(e) => {
        //                     println!("failed to create state object: {:?}", e);
        //                 }
        //             }
        //         }
        //     }
        
        //     tx.commit();        
        // });
    }

    fn get_state_int (&self, entity_id: String, prop: String) -> Option<i64>  {
        todo!("not implemented");

    //     self.get_checked_out_doc_handle().with_doc(|checked_out_doc| {

    //    let state  = match checked_out_doc.get_obj_id(ROOT, "state") {
    //         Some(id) => id,
    //         None => {
    //             println!("invalid document, no state property");
    //             return None
    //         }
    //     };

    //    let entity_id_clone = entity_id.clone();
    //    let entity  = match checked_out_doc.get_obj_id(state, entity_id) {
    //         Some(id) => id,
    //         None => {
    //             println!("entity {:?} does not exist", &entity_id_clone);
    //             return None
    //         }
    //     };

    //     return match checked_out_doc.get_int(entity, prop) {
    //         Some(value) => Some(value),
    //         None =>  None
        
    //     };

    // })
    }

    // these functions below should be extracted into a separate SyncRepo class

    // SYNC

    // needs to be called every frame to process the internal events
    // #[func]
    fn process(&mut self) {

        while let Ok(Some(event)) = self.driver_output_rx.try_next() {
            match event {
                DriverOutputEvent::DocHandleChanged { doc_handle } => {
                    self.doc_handles.insert(doc_handle.document_id(), doc_handle);                
                },
                DriverOutputEvent::BranchesUpdated { branches } => {
                    self.branches = branches;
                    (self.signal_callback)(self.signal_user_data, SIGNAL_BRANCHES_CHANGED.as_ptr(), std::ptr::null(), 0);
                },
                DriverOutputEvent::CheckedOutBranch { branch_doc_id } => {
                    self.checked_out_branch_doc_id = Some(branch_doc_id.clone());
                    let doc_id_c_str = std::ffi::CString::new(format!("{}", &branch_doc_id)).unwrap();
                    (self.signal_callback)(self.signal_user_data, SIGNAL_CHECKED_OUT_BRANCH.as_ptr(),  &doc_id_c_str.as_ptr(), 1);
                },
            }
        }

        // // Process all pending sync events
        // while let Ok(event) = self.sync_event_receiver.try_recv() {
        //     match event {
        //         // this is internal, we don't pass this back to process
        //         SyncEvent::NewDoc { doc_id: _doc_id, doc_handle: _doc_handle } => {}
        //         SyncEvent::DocChanged { doc_id } => {
        //             println!("doc changed event {:?} {:?}", doc_id, self.checked_out_doc_id);
        //             // Check if branches metadata doc changed
        //             if doc_id == self.branches_metadata_doc_id  {                
        //                 (self.signal_callback)(self.signal_user_data, BRANCHES_CHANGED.as_ptr(), std::ptr::null(), 0);
        //             } else if doc_id == self.checked_out_doc_id {
        //                 (self.signal_callback)(self.signal_user_data, SIGNAL_FILES_CHANGED.as_ptr(), std::ptr::null(), 0);
        //             }
        //         }

        //         SyncEvent::CheckedOutBranch { doc_id} => {
        //             self.checked_out_doc_id = doc_id.clone();

        //             let doc_id_c_str = std::ffi::CString::new(format!("{}", doc_id)).unwrap();
        //             (self.signal_callback)(self.signal_user_data, SIGNAL_CHECKED_OUT_BRANCH.as_ptr(), &doc_id_c_str.as_ptr(), 1);
        //         }
        //     }
        // }
    }

}

fn handle_changes(handle: DocHandle) -> impl futures::Stream<Item = Vec<automerge::Patch>> + Send {
    futures::stream::unfold(handle, |doc_handle| async {
        let heads_before = doc_handle.with_doc(|d| d.get_heads().to_vec());
        let _ = doc_handle.changed().await;
        let heads_after = doc_handle.with_doc(|d| d.get_heads().to_vec());
        let diff = doc_handle.with_doc(|d| {
            d.diff(
                &heads_before,
                &heads_after,
                automerge::patches::TextRepresentation::String(TextEncoding::Utf8CodeUnit),
            )
        });

        Some((diff, doc_handle))
    })
}


// return a tuple of the linked_doc_requests_len and linked_doc_handles
// pub(crate) async fn get_linked_docs(repo_handle: &RepoHandle, branch_doc_handle: &DocHandle) -> (usize, Vec<DocHandle>) {
//     let linked_doc_requests = get_linked_docs_of_branch(&branch_doc_handle).into_iter().map(|doc_id| {
//         (doc_id.clone(), repo_handle.request_document(doc_id))
//     }).collect::<Vec<_>>();

//     let linked_doc_requests_len = linked_doc_requests.len();

//     let linked_doc_handles = futures::future::join_all(linked_doc_requests.into_iter().map(|(doc_id, future)| {
//         future.map(move |result| {
//             match result {
//                 Ok(handle) => Some(handle),
//                 Err(e) => {
//                     println!("Failed to load linked doc {:?}: {:?}", doc_id, e);
//                     None
//                 }
//             }
//         })
//     })).await.into_iter().flatten().collect::<Vec<_>>();

//     (linked_doc_requests_len, linked_doc_handles)
// }

// async fn init_godot_project_state(repo_handle: &RepoHandle, doc_handle_map: DocHandleMap, maybe_branches_metadata_doc_id: Option<DocumentId>) -> (DocumentId, DocumentId) {
//     match maybe_branches_metadata_doc_id {
//         None => {
//             // Create new project doc
//             let project_doc_handle = repo_handle.new_document();
//             project_doc_handle.with_doc_mut(|d| {
//                 let mut tx = d.transaction();
//                 let _ = reconcile(
//                     &mut tx,
//                     GodotProjectDoc {
//                         files: HashMap::new(),
//                         state: HashMap::new()
//                     },
//                 );
//                 tx.commit();
//             });
//             let main_branch_doc_id = project_doc_handle.document_id();

//             // Create new branches metadata doc
//             let branches_metadata_doc_handle = repo_handle.new_document();
//             branches_metadata_doc_handle.with_doc_mut(|d| {
//                 let mut tx = d.transaction();
//                 let _ = reconcile(
//                     &mut tx,
//                     BranchesMetadataDoc {
//                         main_doc_id: main_branch_doc_id.to_string(),
//                         branches: HashMap::new(),
//                     },
//                 );
//                 tx.commit();
//             });

//             let branches_metadata_doc_handle_clone = branches_metadata_doc_handle.clone();

//             // save doc handles to the doc_handle_map
//             {
//                 doc_handle_map.add_handle(project_doc_handle.clone());
//                 doc_handle_map.add_handle(branches_metadata_doc_handle.clone());
//             }

//             return (main_branch_doc_id, branches_metadata_doc_handle_clone.document_id());        
            
//         },

//         Some(branches_metadata_doc_id) => {
//             let repo_handle_clone = repo_handle.clone();
//             let branches_metadata_doc_id_clone = branches_metadata_doc_id.clone();

//             let branches_metadata_doc_handle = repo_handle_clone
//                 .request_document(branches_metadata_doc_id)
//                 .await
//                 .unwrap();

//             let branches_metadata: BranchesMetadataDoc =
//                 branches_metadata_doc_handle.with_doc(|d| hydrate(d).unwrap());
            
//             load_new_branch_docs(&branches_metadata, &repo_handle_clone, &doc_handle_map).await;

//             doc_handle_map.add_handle(branches_metadata_doc_handle.clone());

//             let main_branch_doc_id = DocumentId::from_str(&branches_metadata.main_doc_id).unwrap();
//             return (main_branch_doc_id, branches_metadata_doc_id_clone);        
//         }
//     }
// }

// async fn load_new_branch_docs (branches_metadata: &BranchesMetadataDoc, repo_handle: &RepoHandle, doc_handle_map: &DocHandleMap) {        
//     let mut new_branch_doc_ids = branches_metadata.branches.keys().filter_map(|branch_id| {
//        let branch_doc_id = DocumentId::from_str(branch_id).unwrap();
//        match doc_handle_map.get_handle(&branch_doc_id) {
//            Some(_) => None, // ignore branches we already have
//            None => Some(branch_doc_id)    
//        }
//     }).collect::<Vec<_>>();

//     // do we need to load the main branch doc?
//     if doc_handle_map.get_handle(&DocumentId::from_str(&branches_metadata.main_doc_id).unwrap()).is_none() {
//         new_branch_doc_ids.push(DocumentId::from_str(&branches_metadata.main_doc_id).unwrap());
//     }

//     let new_branch_doc_handles = futures::future::join_all(
//         new_branch_doc_ids.iter().map(|doc_id| {
//             repo_handle.request_document(doc_id.clone()).map(move |result| {
//                 match result {
//                     Ok(handle) => Some(handle.clone()),
//                     Err(e) => {
//                         println!("Failed to load branch doc {:?}: {:?}", doc_id.clone(), e);
//                         None
//                     }
//                 }    
//             })       
//         })
//     ).await.into_iter().flatten();
//    let new_branch_doc_handles_clone = new_branch_doc_handles.clone();

//    // Process branch docs to find and request linked documents, then load them in parallel
//    let linked_doc_handles: Vec<DocHandle> = {
//        // First collect all document requests from each branch
//        let linked_doc_requests = new_branch_doc_handles_clone.flat_map(|branch_doc_handle| {                                
//            branch_doc_handle.with_doc(|d| {
//                get_linked_docs_of_branch(&branch_doc_handle)
//                    .into_iter().map(|doc_id| {
//                        (doc_id.clone(), repo_handle.request_document(doc_id))
//                    }).collect::<Vec<_>>()
//            })
//        }).collect::<Vec<_>>();

//        // Execute all requests in parallel and filter out failures
//        futures::future::join_all(linked_doc_requests.into_iter().map(|(doc_id, future)| {
//            future.map(move |result| {
//                match result {
//                    Ok(handle) => Some(handle),
//                    Err(e) => {
//                        println!("Failed to load linked doc {:?}: {:?}", doc_id, e);
//                        None
//                    }
//                }
//            })
//        }))
//        .await
//        .into_iter()
//        .flatten()
//        .collect()                    
//    };                        

//    // Add all handles to the doc_handle_map
//    for handle in new_branch_doc_handles {
//        doc_handle_map.add_handle(handle);
//    }
//    for handle in linked_doc_handles {
//        doc_handle_map.add_handle(handle);
//    }
// }

pub(crate) fn is_branch_doc(branch_doc_handle: &DocHandle) -> bool {
    branch_doc_handle.with_doc(|d| {
        match d.get_obj_id(ROOT, "files") {
            Some(_) => true,
            None => false,
        }
    })
}

// C FFI functions for GodotProject

#[no_mangle]
pub extern "C" fn godot_project_get_fs_doc_id(godot_project: *const GodotProject_rs) -> *const std::os::raw::c_char { 
    let godot_project = unsafe { &*godot_project };
    let fs_doc_id = godot_project.get_doc_id();
    let c_string = std::ffi::CString::new(fs_doc_id).unwrap();
    c_string.into_raw()
}

// free const char * string; rust docs explicitly say you shouldn't attempt to call stdlib's free on a rust-allocated string
#[no_mangle]
pub extern "C" fn godot_project_free_string(s: *const std::os::raw::c_char) {
    unsafe {
        if s.is_null() {
            return;
        }
        drop(std::ffi::CString::from_raw(s as *mut c_char));
    }
}

#[no_mangle]
pub extern "C" fn godot_project_free_vec_string(s: *const *const std::os::raw::c_char, len: u64) {
    unsafe {
        if s.is_null() {
            return;
        }
        let c_strs = std::slice::from_raw_parts(s, len as usize);
        for c_str in c_strs {
            drop(std::ffi::CString::from_raw(*c_str as *mut c_char));
        }
    }
}

#[no_mangle] // free u8 array
pub extern "C" fn godot_project_free_u8_vec(s: *const u8, len: usize) {
    unsafe {
        if s.is_null() {
            return;
        }
        drop(Vec::from_raw_parts(s as *mut u8, len, len));
    }
}



#[no_mangle]
pub extern "C" fn godot_project_create(
    maybe_fs_doc_id: *const std::os::raw::c_char,
    signal_user_data: *mut c_void,
    signal_callback: AutoMergeSignalCallback,
) -> *mut GodotProject_rs {
    let maybe_fs_doc_id = unsafe { std::ffi::CStr::from_ptr(maybe_fs_doc_id) }
        .to_str()
        .unwrap()
        .to_string();
    let godot_project = GodotProject_rs::create(maybe_fs_doc_id, signal_user_data, signal_callback);
    Box::into_raw(Box::new(godot_project))
}

#[no_mangle]
pub extern "C" fn godot_project_stop(godot_project: *mut GodotProject_rs) {
    let godot_project = unsafe { &mut *godot_project };
    godot_project.stop();
}

#[no_mangle]
pub extern "C" fn godot_project_process(godot_project: *mut GodotProject_rs) {
    let godot_project = unsafe { &mut *godot_project };
    godot_project.process();
}

#[no_mangle]
pub extern "C" fn godot_project_save_file(
    godot_project: *const GodotProject_rs,
    path: *const std::os::raw::c_char,
    heads: *const std::os::raw::c_char, // todo: heads should be an vec of strings
    content: *const std::os::raw::c_char,
    content_len: usize,
    binary: bool
) {
    let godot_project = unsafe { &*godot_project };
    let path = unsafe { std::ffi::CStr::from_ptr(path) }
        .to_str()
        .unwrap()
        .to_string();


    let heads_str = unsafe { std::ffi::CStr::from_ptr(heads) }
    .to_str()
    .unwrap()
    .to_string();

    let heads: Option<Vec<ChangeHash>> = if heads_str.is_empty() {
        None
    } else {
        Some(heads_str.split(",").map(|h| ChangeHash::from_str(h).unwrap()).collect())
    };

    if binary {
        let content_u8 = unsafe { std::slice::from_raw_parts(content as *const u8, content_len) };
        let content = StringOrPackedByteArray::PackedByteArray(content_u8.to_vec());
        godot_project.save_file(path, heads, content);
    } else {
        let content = StringOrPackedByteArray::String(unsafe { std::ffi::CStr::from_ptr(content) }
            .to_str()
            .unwrap()
            .to_string());
        godot_project.save_file(path, heads, content);
    }
}


#[no_mangle]
pub extern "C" fn godot_project_destroy(godot_project: *mut GodotProject_rs) {
    unsafe {
        drop(Box::from_raw(godot_project));
    }
}

// takes a pointer to an int for the length and returns a *const std::os::raw::c_char for the array of strings
#[no_mangle]
pub extern "C" fn godot_project_get_branches(godot_project: *mut GodotProject_rs, _len: *mut u64) -> *const *const c_char
{
    let godot_project = unsafe { &mut *godot_project };
    let branches = godot_project.get_branches();

    let c_strs = strings_to_c_strs(&branches);
    let char_stars = to_char_stars(&c_strs).clone();
    //char_stars.into_raw_parts()
    // ignore unstable
    unsafe { *_len = (char_stars.len() / 4) as u64 };
    let ptr = char_stars.as_ptr();
    std::mem::forget(c_strs);
    std::mem::forget(char_stars);
    ptr
}

#[no_mangle]
pub extern "C" fn godot_project_checkout_branch(godot_project: *mut GodotProject_rs, branch_id: *const std::os::raw::c_char) {
    let godot_project = unsafe { &mut *godot_project };
    let branch_id = unsafe { std::ffi::CStr::from_ptr(branch_id) }
        .to_str()
        .unwrap()
        .to_string();
    godot_project.checkout_branch(branch_id);
}

// create_branch
#[no_mangle]
pub extern "C" fn godot_project_create_branch(godot_project: *mut GodotProject_rs, name: *const std::os::raw::c_char) -> *const std::os::raw::c_char {
    let godot_project = unsafe { &mut *godot_project };
    let name = unsafe { std::ffi::CStr::from_ptr(name) }
        .to_str()
        .unwrap()
        .to_string();
    let branch_id = godot_project.create_branch(name);
    let c_string = std::ffi::CString::new(branch_id).unwrap();
    c_string.into_raw()
}

// merge_branch
#[no_mangle]
pub extern "C" fn godot_project_merge_branch(godot_project: *mut GodotProject_rs, branch_id: *const std::os::raw::c_char) {
    let godot_project = unsafe { &mut *godot_project };
    let branch_id = unsafe { std::ffi::CStr::from_ptr(branch_id) }
        .to_str()
        .unwrap()
        .to_string();

    godot_project.merge_branch(branch_id);
}


#[no_mangle]
pub extern "C" fn godot_project_get_checked_out_branch_id(godot_project: *const GodotProject_rs) -> *const std::os::raw::c_char {
    let godot_project = unsafe { &*godot_project };
    let checked_out_branch_id = godot_project.get_checked_out_branch_id();
    let c_string = std::ffi::CString::new(checked_out_branch_id).unwrap();
    c_string.into_raw()
}

#[no_mangle]
// pass back a u8 pointer
pub extern "C" fn godot_project_get_file(godot_project: *const GodotProject_rs, path: *const std::os::raw::c_char, r_len: *mut u64, r_is_binary: *mut u8) -> *const std::os::raw::c_uchar {  
                                                                                                                                                          
                                                                                                                                                          
    let godot_project = unsafe { &*godot_project };
    let path = unsafe { std::ffi::CStr::from_ptr(path) }
        .to_str()
        .unwrap()
        .to_string();
    let file = godot_project.get_file(path.clone());
    match file {
        Some(StringOrPackedByteArray::String(s)) => {
            println!("rust: path {:?} is string", path);
            unsafe {
                r_is_binary.write(0);
                r_len.write(s.len() as u64);
            };
            let c_string = std::ffi::CString::new(s).unwrap();
            // cast to u8
            c_string.into_raw() as *const u8
        },
        Some(StringOrPackedByteArray::PackedByteArray(bytes)) => {
            println!("rust: path {:?} is packed byte array", path);
            unsafe {
                r_is_binary.write(1);
                r_len.write(bytes.len() as u64);
            };
            let ptr = bytes.as_ptr();
            std::mem::forget(bytes);
            ptr
        },
        None => {
            println!("rust: path {:?} is none", path);
            std::ptr::null()
        }
    }
}

//list_all_files
#[no_mangle]
pub extern "C" fn godot_project_list_all_files(godot_project: *const GodotProject_rs, _len: *mut u64) -> *const *const c_char {
    let godot_project = unsafe { &*godot_project };
    let files = godot_project.list_all_files();

    let c_strs = strings_to_c_strs(&files);
    let char_stars = to_char_stars(&c_strs);
    let len = (char_stars.len()) as u64;
    unsafe { *_len = len };

    let ptr = char_stars.as_ptr();
    std::mem::forget(c_strs);
    std::mem::forget(char_stars);
    ptr
}

//get_heads
#[no_mangle]
pub extern "C" fn godot_project_get_heads(godot_project: *const GodotProject_rs, _len: *mut u64) -> *const *const c_char {
    let godot_project = unsafe { &*godot_project };
    let heads = godot_project.get_heads();

    let c_strs = strings_to_c_strs(&heads);
    let char_stars = to_char_stars(&c_strs);
    let len = (char_stars.len()) as u64;
    unsafe { *_len = len };

    let ptr = char_stars.as_ptr();
    std::mem::forget(c_strs);
    std::mem::forget(char_stars);
    ptr
}
//get_changes

#[no_mangle]
pub extern "C" fn godot_project_get_changes(godot_project: *const GodotProject_rs, _len: *mut u64) -> *const *const c_char {
    let godot_project = unsafe { &*godot_project };
    let changes = godot_project.get_changes();

    let c_strs = strings_to_c_strs(&changes);
    let char_stars = to_char_stars(&c_strs);
    let len = (char_stars.len()) as u64;
    unsafe { *_len = len };

    let ptr = char_stars.as_ptr();
    std::mem::forget(c_strs);
    std::mem::forget(char_stars);
    ptr
}

// takes in a 

// state sync function

#[no_mangle]
pub extern "C" fn godot_project_get_state_int(godot_project: *const GodotProject_rs, entity_id: *const std::os::raw::c_char, prop: *const std::os::raw::c_char) -> *const i64 {
    let godot_project = unsafe { &*godot_project };
    let entity_id = unsafe { std::ffi::CStr::from_ptr(entity_id) }
        .to_str()
        .unwrap()
        .to_string();
    let prop = unsafe { std::ffi::CStr::from_ptr(prop) }
        .to_str()
        .unwrap()
        .to_string();
    
    match godot_project.get_state_int(entity_id, prop) {
        Some(value) => {
            let boxed = Box::new(value);
            Box::into_raw(boxed)
        },
        None => {
            println!("none");
            return std::ptr::null()
        }
    }
}


#[no_mangle]
pub extern "C" fn godot_project_set_state_int(godot_project: *const GodotProject_rs, entity_id: *const std::os::raw::c_char, prop: *const std::os::raw::c_char, value: i64) {
    let godot_project = unsafe { &*godot_project };
    let entity_id = unsafe { std::ffi::CStr::from_ptr(entity_id) }
        .to_str()
        .unwrap()
        .to_string();
    let prop = unsafe { std::ffi::CStr::from_ptr(prop) }
        .to_str()
        .unwrap()
        .to_string();

    godot_project.set_state_int(entity_id, prop, value);
}
