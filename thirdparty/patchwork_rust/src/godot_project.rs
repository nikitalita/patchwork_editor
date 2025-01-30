use std::{
    collections::HashMap,
    str::FromStr,
    sync::{
        mpsc::{Receiver, Sender},
        Arc, Mutex,
    },
};
use ::safer_ffi::prelude::*;

use automerge::{patches::TextRepresentation, transaction::Transactable, Automerge, Change, ChangeHash, ObjType, PatchLog, ReadDoc, ROOT};
use automerge_repo::{tokio::FsStorage, ConnDirection, DocHandle, DocumentId, Repo, RepoHandle};
use autosurgeon::{bytes, hydrate, reconcile, Hydrate, Reconcile};
use futures::StreamExt;
use std::ffi::c_void;
use std::os::raw::c_char;
// use godot::prelude::*;
use tokio::{net::TcpStream, runtime::Runtime};

use crate::{doc_utils::SimpleDocReader, doc_handle_map::DocHandleMap};

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
struct BranchesMetadataDoc {
    main_doc_id: String,
    branches: HashMap<String, Branch>,
}

#[derive(Debug, Clone, Reconcile, Hydrate, PartialEq)]
struct Branch {
    name: String,
    id: String,
    is_merged: bool
}

#[derive(Clone)]
enum SyncEvent {
    DocChanged { doc_id: DocumentId },
}


#[derive(Debug, Clone, Reconcile, Hydrate, PartialEq)]
pub enum StringOrPackedByteArray {
    String(String),
    PackedByteArray(Vec<u8>),
}

// #[derive(GodotClass)]
// #[class(no_init, base=Node)]
pub struct GodotProject_rs {
    // base: Base<Node>,
    runtime: Runtime,
    repo_handle: RepoHandle,
    branches_metadata_doc_id: DocumentId,
    checked_out_doc_id: Arc<Mutex<Option<DocumentId>>>,
    doc_handle_map: DocHandleMap,
    signal_user_data: *mut c_void,
    signal_callback: AutoMergeSignalCallback,
    sync_event_receiver: Receiver<SyncEvent>,
}

const SERVER_URL: &str = "104.131.179.247:8080";
static BRANCHES_CHANGED: std::sync::LazyLock<std::ffi::CString> = std::sync::LazyLock::new(|| std::ffi::CString::new("branches_changed").unwrap());
static SIGNAL_FILES_CHANGED: std::sync::LazyLock<std::ffi::CString> = std::sync::LazyLock::new(|| std::ffi::CString::new("files_changed").unwrap());
static SIGNAL_CHECKED_OUT_BRANCH: std::sync::LazyLock<std::ffi::CString> = std::sync::LazyLock::new(|| std::ffi::CString::new("checked_out_branch").unwrap());
static SIGNAL_FILE_CHANGED: std::sync::LazyLock<std::ffi::CString> = std::sync::LazyLock::new(|| std::ffi::CString::new("file_changed").unwrap());
static SIGNAL_STARTED: std::sync::LazyLock<std::ffi::CString> = std::sync::LazyLock::new(|| std::ffi::CString::new("started").unwrap());

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
        let runtime: Runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = runtime.enter();

        let _ = tracing_subscriber::fmt::try_init();

        // todo: store in project folder
        let storage = FsStorage::open("/tmp/automerge-godot-data").unwrap();
        let repo = Repo::new(None, Box::new(storage));
        let repo_handle = repo.run();

        let (new_doc_tx, new_doc_rx) = futures::channel::mpsc::unbounded();

        let doc_handle_map = DocHandleMap::new(new_doc_tx.clone());

        let checked_out_doc_id = Arc::new(Mutex::new(None));

        let branches_metadata_doc_id = if maybe_branches_metadata_doc_id.is_empty() {
            // Create new project doc
            let project_doc_handle = repo_handle.new_document();
            project_doc_handle.with_doc_mut(|d| {
                let mut tx = d.transaction();
                let _ = reconcile(
                    &mut tx,
                    GodotProjectDoc {
                        files: HashMap::new(),
                        state: HashMap::new()
                    },
                );
                tx.commit();
            });
            let project_doc_id = project_doc_handle.document_id();

            // Create new branches metadata doc
            let branches_metadata_doc_handle = repo_handle.new_document();
            branches_metadata_doc_handle.with_doc_mut(|d| {
                let mut tx = d.transaction();
                let _ = reconcile(
                    &mut tx,
                    BranchesMetadataDoc {
                        main_doc_id: project_doc_id.to_string(),
                        branches: HashMap::new(),
                    },
                );
                tx.commit();
            });

            let project_doc_id_clone = project_doc_id.clone();
            let branches_metadata_doc_handle_clone = branches_metadata_doc_handle.clone();

            // Add both docs to the states
            {
                doc_handle_map.add_handle(project_doc_handle.clone());
                doc_handle_map.add_handle(branches_metadata_doc_handle.clone());
            }

            {
                let mut checked_out = checked_out_doc_id.lock().unwrap();
                *checked_out = Some(project_doc_id_clone.clone());
            }

            branches_metadata_doc_handle_clone.document_id()
        } else {
            let branches_metadata_doc_id =
                DocumentId::from_str(&maybe_branches_metadata_doc_id).unwrap();
            let branches_metadata_doc_id_clone = branches_metadata_doc_id.clone();
            let branches_metadata_doc_id_clone_2 = branches_metadata_doc_id.clone();

            let repo_handle_clone = repo_handle.clone();
            let doc_handle_map = doc_handle_map.clone();
            let checked_out_doc_id = checked_out_doc_id.clone();

            runtime.spawn(async move {
                let repo_handle_result = repo_handle_clone
                    .request_document(branches_metadata_doc_id)
                    .await
                    .unwrap();

                let branches_metadata: BranchesMetadataDoc =
                    repo_handle_result.with_doc(|d| hydrate(d).unwrap());

                let main_doc_id = DocumentId::from_str(&branches_metadata.main_doc_id).unwrap();
                let main_doc_id_clone = main_doc_id.clone();

                // Request main branch doc
                let main_doc_handle_result = repo_handle_clone
                    .request_document(main_doc_id)
                    .await
                    .unwrap();

                // Request all other branch docs
                let mut other_branches = Vec::new();
                for (branch_id, _) in &branches_metadata.branches {
                    let branch_doc_id = DocumentId::from_str(branch_id).unwrap();
                    let branch_doc_handle = repo_handle_clone
                        .request_document(branch_doc_id.clone())
                        .await
                        .unwrap();
                    other_branches.push((branch_doc_id, branch_doc_handle));
                }

                // Add both docs to the states
                {
                    // Add main doc
                    doc_handle_map.add_handle(main_doc_handle_result.clone());

                    // Add other branches
                    for (_, branch_doc_handle) in other_branches {
                        doc_handle_map.add_handle(branch_doc_handle);
                    }

                    // Add branches metadata doc
                    doc_handle_map.add_handle(repo_handle_result);
                }

                let mut checked_out = checked_out_doc_id.lock().unwrap();
                *checked_out = Some(main_doc_id_clone);
            });

            branches_metadata_doc_id_clone_2
        };

        let (sync_event_sender, sync_event_receiver) = std::sync::mpsc::channel();

        // Spawn connection task
        Self::spawn_connection_task(&runtime, repo_handle.clone());

        // Spawn sync task for all doc handles
        Self::spawn_sync_task(
            &runtime,
            new_doc_rx,
            doc_handle_map.clone(),
            sync_event_sender.clone(),
        );

        /*return Gd::from_init_fn(|base| Self */ GodotProject_rs {
            // base,
            runtime,
            repo_handle,
            branches_metadata_doc_id,
            checked_out_doc_id,
            doc_handle_map,
            sync_event_receiver,
            signal_user_data,
            signal_callback,
        }/* );*/
    }

    // PUBLIC API

    // #[func]
    fn stop(&self) {
        // todo: is this right?
        unsafe {
            let runtime = std::ptr::read(&self.runtime);
            runtime.shutdown_background();
        }
    }

    // #[func]
    fn get_doc_id(&self) -> String {
        return self.branches_metadata_doc_id.to_string();
    }

    // #[func]
    // fn get_heads(&self) -> Array<Variant> /* String[] */ {
    //     let checked_out_doc_id = self.get_checked_out_doc_id();

    //     let doc = self.doc_state_map.get_doc(&checked_out_doc_id).unwrap_or_else(|| {
    //         panic!(
    //             "Failed to get doc for checked out doc id: {}",
    //             &checked_out_doc_id
    //         )
    //     });

    //     let heads = doc.get_heads();

    //     return heads
    //         .to_vec()
    //         .iter()
    //         .map(|h| h.to_string().to_variant())
    //         .collect::<Array<Variant>>();
    // }
    fn get_heads(&self) -> Vec<String> /* String[] */ {
        let checked_out_doc_id = self.get_checked_out_doc_id();

        let doc = self.doc_handle_map.get_doc(&checked_out_doc_id).unwrap_or_else(|| {
            panic!(
                "Failed to get doc for checked out doc id: {}",
                &checked_out_doc_id
            )
        });

        let heads = doc.get_heads();

        return heads
            .to_vec()
            .iter()
            .map(|h| h.to_string())
            .collect::<Vec<String>>();
    }


    // // #[func]
    // fn list_all_files(&self) -> Array<Variant> /* String[] */ {
    //     let doc = self
    //         .doc_state_map
    //         .get_doc(&self.get_checked_out_doc_id())
    //         .unwrap_or_else(|| panic!("Failed to get checked out doc"));

    // let files = match doc.get_obj_id(ROOT, "files") {
    // Some(files) => files,
    // _ => {
    // return array![];
    // }
    // };
    // 
    // 
    // let keys = doc.keys(files).collect::<Vec<String>>();
    // return keys.into_iter().map(|k| k.to_variant()).collect::<Array<Variant>>();
    // }
    // #[func]
    fn list_all_files(&self) -> Vec<String> /* String[] */ {
        let doc = self
            .doc_handle_map
            .get_doc(&self.get_checked_out_doc_id())
            .unwrap_or_else(|| panic!("Failed to get checked out doc"));


        let files = match doc.get_obj_id(ROOT, "files") {
            Some(files) => files,
            _ => {
                return vec![];
            }
        };
        doc.keys(files).collect::<Vec<String>>()
    }

    // #[func]
    // fn get_file(&self, path: String) -> Variant /* String? */ {
    //     let doc = self
    //         .doc_state_map
    //         .get_doc(&self.get_checked_out_doc_id())
    //         .unwrap_or_else(|| panic!("Failed to get checked out doc"));


    // does the file exist?
    // let file_entry = match doc.get(files, path) {
    // Ok(Some((automerge::Value::Object(ObjType::Map), file_entry))) => file_entry,
    // _ => return Variant::nil(),
    // };
    // 
    // 
    // // try to read file as text
    // match doc.get(&file_entry, "content") {
    // Ok(Some((automerge::Value::Object(ObjType::Text), content))) => {
    // match doc.text(content) {
    // Ok(text) => return text.to_variant(),
    // Err(_) => {}
    // }
    // },
    // _ => {}
    // }
    // 
    // // ... otherwise try to read as linked binary doc
    // 
    // 
    // if let Some(url) = doc.get_string(&file_entry, "url") {
    // 
    // // parse doc url
    // let doc_id = match parse_automerge_url(&url) {
    // Some(url) => url,
    // _ => return Variant::nil()
    // };
    // 
    // // read content doc
    // let content_doc = match self.doc_state_map.get_doc(&doc_id) {
    // Some(doc) => doc,
    // _ => return Variant::nil()
    // };
    // 
    // // get content of binary file
    // if let Some(bytes) = content_doc.get_bytes(ROOT, "content") {
    // return bytes.to_variant()
    // };
    // };
    // 
    // // finally give up
    // return Variant::nil()
    // }
    fn get_file(&self, path: String) -> Option<StringOrPackedByteArray> {
        let doc = self
            .doc_handle_map
            .get_doc(&self.get_checked_out_doc_id())
            .unwrap_or_else(|| panic!("Failed to get checked out doc"));

        let files = doc.get(ROOT, "files").unwrap().unwrap().1;


        // does the file exist?
        let file_entry = match doc.get(files, path) {
            Ok(Some((automerge::Value::Object(ObjType::Map), file_entry))) => file_entry,
            _ => return None,
        };


        // try to read file as text
        match doc.get(&file_entry, "content") {
            Ok(Some((automerge::Value::Object(ObjType::Text), content))) => {
                match doc.text(content) {
                    Ok(text) => return Some(StringOrPackedByteArray::String(text.to_string())),
                    Err(_) => {}
                }
            },
            _ => {}
        }

        // ... otherwise try to read as linked binary doc


        if let Some(url) = doc.get_string(&file_entry, "url") {

            // parse doc url
            let doc_id = match parse_automerge_url(&url) {
                Some(url) => url,
                _ => return None
            };

            // read content doc
            let content_doc = match self.doc_handle_map.get_doc(&doc_id) {
                Some(doc) => doc,
                _ => return None
            };

            // get content of binary file
            if let Some(bytes) = content_doc.get_bytes(ROOT, "content") {
                return Some(StringOrPackedByteArray::PackedByteArray(bytes));
            };
        };

        // finally give up
        return None
    }


    // // #[func]
    // fn get_file_at(&self, path: String, heads: Array<Variant> /* String[] */) -> Variant /* String? */
    // {
    //     let doc = self
    //         .doc_state_map
    //         .get_doc(&self.get_checked_out_doc_id())
    //         .unwrap_or_else(|| panic!("Failed to get checked out doc"));

    //     let heads: Vec<ChangeHash> = heads
    //         .iter_shared()
    //         .map(|h| ChangeHash::from_str(h.to_string().as_str()).unwrap())
    //         .collect();

    //     let files = doc.get(ROOT, "files").unwrap().unwrap().1;

    //     return match doc.get_at(files, path, &heads) {
    //         Ok(Some((value, _))) => value.into_string().unwrap_or_default().to_variant(),
    //         _ => Variant::nil(),
    //     };
    // }
    // #[func]
    fn get_file_at(&self, path: String, heads: Vec<String> /* String[] */) -> Option<String> /* String? */
    {
        let doc = self
            .doc_handle_map
            .get_doc(&self.get_checked_out_doc_id())
            .unwrap_or_else(|| panic!("Failed to get checked out doc"));

        let heads: Vec<ChangeHash> = heads
            .iter()
            .map(|h| ChangeHash::from_str(h.to_string().as_str()).unwrap())
            .collect();

        let files = doc.get(ROOT, "files").unwrap().unwrap().1;

        return match doc.get_at(files, path, &heads) {
            Ok(Some((value, _))) => Some(value.into_string().unwrap_or_default()),
            _ => None,
        };
    }

    // #[func]
    // fn get_changes(&self) -> Array<Variant> /* String[]  */ {
    //     let doc = self
    //         .doc_state_map
    //         .get_doc(&self.get_checked_out_doc_id())
    //         .unwrap_or_else(|| panic!("Failed to get checked out doc"));

    //     doc.get_changes(&[])
    //         .to_vec()
    //         .iter()
    //         .map(|c| c.hash().to_string().to_variant())
    //         .collect::<Array<Variant>>()
    // }
    fn get_changes(&self) -> Vec<String> /* String[]  */ {
        let doc = self
            .doc_handle_map
            .get_doc(&self.get_checked_out_doc_id())
            .unwrap_or_else(|| panic!("Failed to get checked out doc"));

        doc.get_changes(&[])
            .to_vec()
            .iter()
            .map(|c| c.hash().to_string())
            .collect::<Vec<String>>()
    }

    // #[func]
    fn save_file(&self, path: String, heads: Option<Vec<ChangeHash>>, content: StringOrPackedByteArray) {
        let checked_out_doc_id = self.get_checked_out_doc_id();

        let checked_out_doc_handle = self.get_doc_handle(checked_out_doc_id.clone());

        if let Some(project_doc_handle) = checked_out_doc_handle {
            project_doc_handle.with_doc_mut(|d| {
                let mut tx = match heads {
                    Some(heads) => {
                        println!("change at heads {:?}", heads);

                        d.transaction_at(PatchLog::inactive(TextRepresentation::String), &heads)
                    },
                    None => {
                        println!("don't have heads");
                        d.transaction()
                    }
                };

                let files = match tx.get(ROOT, "files") {
                    Ok(Some((automerge::Value::Object(ObjType::Map), files))) => files,
                    _ => panic!("Invalid project doc, doesn't have files map"),
                };

                match content {
                    StringOrPackedByteArray::String(content) => {
                        println!("write string {:}", path);

                        // get existing file url or create new one                        
                        let file_entry = match tx.get(&files, &path) {
                            Ok(Some((automerge::Value::Object(ObjType::Map), file_entry))) => file_entry,
                            _ => tx.put_object(files, &path, ObjType::Map).unwrap()
                        };

                        // delete url in file entry if it previously had one
                        if let Ok(Some((_, _))) = tx.get(&file_entry, "url") {
                            let _ = tx.delete(&file_entry, "url");
                        }

                        // either get existing text or create new text
                        let content_key = match tx.get(&file_entry, "content") {
                            Ok(Some((automerge::Value::Object(ObjType::Text), content))) => content,
                            _ => tx.put_object(&file_entry, "content", ObjType::Text).unwrap(),
                        };
                        let _ = tx.update_text(&content_key, &content);
                    },
                    StringOrPackedByteArray::PackedByteArray(content) => {
                        println!("write binary {:}", path);

                        // create content doc
                        let content_doc_id = self.create_doc(|d| {
                            let mut tx = d.transaction();
                            let _ = tx.put(ROOT, "content", content.to_vec());
                            tx.commit();
                        });

                        // write url to content doc into project doc
                        let file_entry = tx.put_object(files, path, ObjType::Map);
                        let _ = tx.put(file_entry.unwrap(), "url", format!("automerge:{}", content_doc_id));
                    },
                }

                tx.commit();
            });
        } else {
            println!("too early {:?}", path)
        }
    }

    fn merge_branch(&self, branch_id: String) {
        let branches_metadata_doc = self
            .doc_handle_map
            .get_doc(&self.branches_metadata_doc_id)
            .unwrap();

      
        // merge branch into main

        let branch_doc = self.doc_handle_map.get_handle(&DocumentId::from_str(&branch_id).unwrap()).unwrap();

        let main_doc_id = DocumentId::from_str(&branches_metadata_doc.get_string(ROOT, "main_doc_id").unwrap()).unwrap();

        branch_doc.with_doc_mut(|branch_doc| {
            self.update_doc(main_doc_id, |d| {
                d.merge(branch_doc);
            });
        });
        
        // mark branch as merged

        let mut branches_metadata: BranchesMetadataDoc = hydrate(&branches_metadata_doc)
        .unwrap_or_else(|e| panic!("Failed to hydrate branches metadata doc: {}", e));

        let branch = branches_metadata.branches.get_mut(&branch_id).unwrap();
        branch.is_merged = true;

        self.update_doc(self.branches_metadata_doc_id.clone(), |d| {
            let mut tx = d.transaction();
            reconcile(&mut tx, branches_metadata).unwrap();
            tx.commit();
        });
    }

    // #[func]
    fn create_branch(&self, name: String) -> String {
        let branches_metadata_doc = self
            .doc_handle_map
            .get_doc(&self.branches_metadata_doc_id)
            .unwrap_or_else(|| panic!("Failed to load branches metadata doc"));

        let mut branches_metadata: BranchesMetadataDoc = hydrate(&branches_metadata_doc)
            .unwrap_or_else(|e| panic!("Failed to hydrate branches metadata doc: {}", e));

        let main_doc_id = DocumentId::from_str(&branches_metadata.main_doc_id).unwrap();
        let new_doc_id = self.clone_doc(main_doc_id);

        branches_metadata.branches.insert(
            new_doc_id.to_string(),
            Branch {
                is_merged: false,
                name,
                id: new_doc_id.to_string(),
            },
        );

        self.update_doc(self.branches_metadata_doc_id.clone(), |d| {
            let mut tx = d.transaction();
            reconcile(&mut tx, branches_metadata).unwrap();
            tx.commit();
        });

        new_doc_id.to_string()
    }

    // #[func]
    fn checkout_branch(&mut self, branch_id: String) {
        let doc_id = if branch_id == "main" {
            let branches_metadata_doc = self
                .doc_handle_map
                .get_doc(&self.branches_metadata_doc_id)
                .unwrap_or_else(|| panic!("couldn't load branches metadata doc {}", branch_id));

            let branches_metadata: BranchesMetadataDoc =
                hydrate(&branches_metadata_doc).expect("failed to hydrate branches metadata doc");

            DocumentId::from_str(&branches_metadata.main_doc_id).unwrap()
        } else {
            DocumentId::from_str(&branch_id).unwrap()
        };

        {
            let mut checked_out = self.checked_out_doc_id.lock().unwrap();
            *checked_out = Some(doc_id);
        } // Release the lock before emitting signal

        // self.base_mut()
        //     .emit_signal("checked_out_branch", &[branch_id.to_variant()]);
        let slc = &[branch_id.as_str()];
        let arg_cstrs = to_c_strs(slc);
        let args = to_char_stars(&arg_cstrs);

        // @paul: passing args causes error so I'm not passing the checked out branch
        (self.signal_callback)(self.signal_user_data, SIGNAL_CHECKED_OUT_BRANCH.as_ptr(), std::ptr::null(), 0);

    }

    // #[func]
    // fn get_branches(&self) -> Array<Variant> /* { name: String, id: String }[] */ {
    //     let maybe_branches_metadata_doc = self.get_doc(self.branches_metadata_doc_id.clone());

    //     let branches_metadata_doc = match maybe_branches_metadata_doc {
    //         Some(doc) => doc,
    //         None => {
    //             panic!("couldn't load branches_metadata_doc");
    //         }
    //     };

    //     let branches_metadata: BranchesMetadataDoc = hydrate(&branches_metadata_doc).unwrap();

    //     let mut branches = array![];

    //     // Add main branch
    //     branches.push(
    //         &dict! {
    //             "name": "main",
    //             "id": "main"
    //         }
    //         .to_variant(),
    //     );

    //     // Add other branches
    //     for (id, branch) in branches_metadata.branches {
    //         branches.push(
    //             &dict! {
    //                 "name": branch.name,
    //                 "id": id
    //             }
    //             .to_variant(),
    //         );
    //     }

    //     branches
    // }
    fn get_branches(&self) -> Vec<String> /* { name: String, id: String }[] */ {
        let maybe_branches_metadata_doc = self.get_doc(self.branches_metadata_doc_id.clone());

        let branches_metadata_doc = match maybe_branches_metadata_doc {
            Some(doc) => doc,
            None => {
                panic!("couldn't load branches_metadata_doc");
            }
        };

        let branches_metadata: BranchesMetadataDoc = hydrate(&branches_metadata_doc).unwrap();

        let mut branches: Vec<String> = vec!("name".to_string(), "main".to_string(),  "id".to_string(), "main".to_string());
        
        // Add other branches
        for (id, branch) in branches_metadata.branches {
            branches.push("name".to_string());
            branches.push(branch.name);
            branches.push("id".to_string());
            branches.push(id);
        }

        branches
    }

    // #[func]
    fn get_checked_out_branch_id(&self) -> String {
        return self
            .checked_out_doc_id
            .lock()
            .unwrap()
            .clone()
            .unwrap()
            .to_string();
    }

    // State api

    fn set_state_int (&self, entity_id: String, prop: String, value: i64) {
        let maybe_checked_out_doc_handle = self.doc_handle_map.get_handle(&self.checked_out_doc_id.lock().unwrap().clone().unwrap());

        let checked_out_doc_handle = match maybe_checked_out_doc_handle {
            Some(doc) => doc,
            _ => {
                println!("couldn't load checked out doc");
                return
            }   
        };

        
        checked_out_doc_handle.with_doc_mut(|d| {
            let mut tx = d.transaction();
            let state = match tx.get_obj_id(ROOT, "state") {
                Some(id) => id,
                _ => {
                    println!("failed to load state");
                    return 
                }
            };


            match tx.get_obj_id(&state, &entity_id) {
                Some(id) => {
                    let _ = tx.put(id, prop, value);
                },                
                
                None => {
                    match tx.put_object(state, &entity_id, ObjType::Map) {
                        Ok(id) => {
                            let _ = tx.put(id, prop, value);
                        },
                        Err(e) => {
                            println!("failed to create state object: {:?}", e);
                        }
                    }
                }
            }
        
            tx.commit();        
        });
    }

    fn get_state_int (&self, entity_id: String, prop: String) -> Option<i64>  {
        let maybe_checked_out_doc = self.doc_handle_map.get_doc(&self.checked_out_doc_id.lock().unwrap().clone().unwrap());

        let checked_out_doc = match maybe_checked_out_doc {
            Some(doc) => doc,
            _ => {
                println!("couldn't load checked out doc");
                return None
            }   
        };

       let state  = match checked_out_doc.get_obj_id(ROOT, "state") {
            Some(id) => id,
            None => {
                println!("invalid document, no state property");
                return None
            }
        };

       let entity_id_clone = entity_id.clone();
       let entity  = match checked_out_doc.get_obj_id(state, entity_id) {
            Some(id) => id,
            None => {
                println!("entity {:?} does not exist", &entity_id_clone);
                return None
            }
        };

        return match checked_out_doc.get_int(entity, prop) {
            Some(value) => Some(value),
            None =>  None
        
        };
    }

    // these functions below should be extracted into a separate SyncRepo class

    // SYNC

    // needs to be called every frame to process the internal events
    // #[func]
    fn process(&mut self) {
        let checked_out_doc_id = match self.checked_out_doc_id.lock().unwrap().clone() {
            Some(id) => id,
            None => return,
        };

        // Process all pending sync events
        while let Ok(event) = self.sync_event_receiver.try_recv() {
            match event {
                SyncEvent::DocChanged { doc_id } => {
                    println!("doc changed event {:?} {:?}", doc_id, checked_out_doc_id);

                    // Check if branches metadata doc changed
                    if doc_id == self.branches_metadata_doc_id {

                        // load branches metadata doc
                        let doc = self
                            .doc_handle_map
                            .get_doc(&self.branches_metadata_doc_id)
                            .unwrap_or_else(|| panic!("Failed to get branches metadata doc"));
                        
                        let branches_metadata: BranchesMetadataDoc = hydrate(&doc)
                            .unwrap_or_else(|e| panic!("Failed to hydrate branches metadata doc: {:?}", e));

                        // check if it has new branches so we can load them
                        for (branch_doc_id, _) in branches_metadata.branches {
                            let branch_doc_id = DocumentId::from_str(&branch_doc_id).unwrap();
                            if self.doc_handle_map.get_handle(&branch_doc_id).is_none() {
                                self.request_doc(branch_doc_id);                            
                            }
                        }

                        // self.base_mut().emit_signal("branches_changed", &[]);
                        (self.signal_callback)(self.signal_user_data, BRANCHES_CHANGED.as_ptr(), std::ptr::null(), 0);
                    }
                    // Check if checked out doc changed
                    else if doc_id == checked_out_doc_id {
                        (self.signal_callback)(self.signal_user_data, SIGNAL_FILES_CHANGED.as_ptr(), std::ptr::null(), 0);

                        // self.base_mut().emit_signal("files_changed", &[]);
                    }
                }
            }
        }
    }

    fn spawn_connection_task(runtime: &Runtime, repo_handle: RepoHandle) {
        let repo_handle_clone = repo_handle.clone();
        runtime.spawn(async move {
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

    fn spawn_sync_task(
        runtime: &Runtime,
        mut new_docs: futures::channel::mpsc::UnboundedReceiver<DocHandle>,
        doc_handle_map: DocHandleMap,
        sync_event_sender: Sender<SyncEvent>,
    ) {
        let initial_handles = doc_handle_map
            .current_handles();

        runtime.spawn(async move {
            // This is a stream of SyncEvent
            let mut all_doc_changes = futures::stream::SelectAll::new();

            // First add a stream for all the initial documents to the SelectAll
            for doc_handle in initial_handles {
                let doc_id = doc_handle.document_id();
                let doc_handle = doc_handle.clone();

                let change_stream = handle_changes(doc_handle.clone()).filter_map(move |diff| {
                    let doc_id = doc_id.clone();
                    async move {
                        if diff.is_empty() {
                            None
                        } else {
                            Some(SyncEvent::DocChanged { doc_id: doc_id.clone() } )
                        }
                    }
                });

                all_doc_changes.push(change_stream.boxed());
            }

            // Now, drive the SelectAll and also wait for any new documents to  arrive and add
            // them to the selectall
            loop {
                futures::select! {
                    sync_event = all_doc_changes.select_next_some() => {
                        // emit change event
                        sync_event_sender.send(sync_event).unwrap();                    
                    }
                    new_doc = new_docs.select_next_some() => {
                        let doc_id = new_doc.document_id();
                        let doc_handle = new_doc.clone();

                        println!("new doc!!!");

                        let change_stream = handle_changes(doc_handle.clone()).filter_map(move |diff| {
                            let doc_id = doc_id.clone();
                            async move {
                                if diff.is_empty() {
                                    None
                                } else {
                                    Some(SyncEvent::DocChanged { doc_id: doc_id.clone() } )
                                }
                            }
                        });

                        all_doc_changes.push(change_stream.boxed());
                    }
                }
            }
        });
    }

    // DOC ACCESS + MANIPULATION

    fn update_doc<F>(&self, doc_id: DocumentId, f: F)
    where
        F: FnOnce(&mut Automerge),
    {
        if let Some(doc_handle) = self.doc_handle_map.get_handle(&doc_id) {
            doc_handle.with_doc_mut(f);
        }
    }

    fn create_doc<F>(&self, f: F) -> DocumentId
    where
        F: FnOnce(&mut Automerge),
    {
        let doc_handle = self.repo_handle.new_document();
        let doc_id = doc_handle.document_id();

        self.doc_handle_map.add_handle(doc_handle.clone());

        self.update_doc(doc_id.clone(), f);

        doc_id
    }


    fn request_doc(&self, doc_id: DocumentId) {
        let repo_handle = self.repo_handle.clone();
        let doc_handle_map = self.doc_handle_map.clone();

        self.runtime.spawn(async move {
            let repo_handle_result = repo_handle
                .request_document(doc_id);
            
            let doc_handle = repo_handle_result.await.unwrap();
            doc_handle_map.add_handle(doc_handle);
        });
    }

    fn clone_doc(&self, doc_id: DocumentId) -> DocumentId {
        let new_doc_handle = self.repo_handle.new_document();
        let new_doc_id = new_doc_handle.document_id();
        let doc_handle = self.get_doc_handle(doc_id.clone());

        let changes_count = doc_handle
            .clone()
            .unwrap()
            .with_doc(|d| d.get_changes(&[]).len());

        println!("changes {:?}", changes_count);

        let doc_handle = doc_handle.unwrap_or_else(|| panic!("Couldn't clone doc_id: {}", &doc_id));
        let _ = doc_handle
            .with_doc_mut(|mut main_d| new_doc_handle.with_doc_mut(|d| d.merge(&mut main_d)));

        self.doc_handle_map.add_handle(new_doc_handle.clone());

        new_doc_id
    }

    fn get_doc(&self, id: DocumentId) -> Option<Automerge> {
        self.doc_handle_map.get_doc(&id.into())
    }

    fn get_doc_handle(&self, id: DocumentId) -> Option<DocHandle> {
        self.doc_handle_map.get_handle(&id.into())
    }

    fn get_checked_out_doc_id(&self) -> DocumentId {
        return self.checked_out_doc_id.lock().unwrap().clone().unwrap();
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
                automerge::patches::TextRepresentation::String,
            )
        });

        Some((diff, doc_handle))
    })
}


fn parse_automerge_url(url: &str) -> Option<DocumentId> {
    const PREFIX: &str = "automerge:";
    if !url.starts_with(PREFIX) {
        return None;
    }

    let hash = &url[PREFIX.len()..];
    DocumentId::from_str(hash).ok()
}





// C FFI functions for GodotProject

#[no_mangle]
pub extern "C" fn godot_project_get_branch_doc_id(godot_project: *const GodotProject_rs) -> *const std::os::raw::c_char {
    let godot_project = unsafe { &*godot_project };
    let branch_doc_id = godot_project.get_checked_out_branch_id();
    let c_string = std::ffi::CString::new(branch_doc_id).unwrap();
    c_string.into_raw()
}


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
    let file = godot_project.get_file(path);
    match file {
        Some(StringOrPackedByteArray::String(s)) => {
            unsafe {
                r_is_binary.write(0);
                r_len.write(s.len() as u64);
            };
            let c_string = std::ffi::CString::new(s).unwrap();
            // cast to u8
            c_string.into_raw() as *const u8
        },
        Some(StringOrPackedByteArray::PackedByteArray(bytes)) => {
            unsafe {
                r_is_binary.write(1);
                r_len.write(bytes.len() as u64);
            };
            let ptr = bytes.as_ptr();
            std::mem::forget(bytes);
            ptr
        },
        None => std::ptr::null()
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
