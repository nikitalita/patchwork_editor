use std::{
    str::FromStr,
    sync::mpsc::{channel, Receiver, Sender},
};
use std::collections::HashMap;
use automerge::{ChangeHash, Patch, ScalarValue};
use autosurgeon::{hydrate, reconcile};
// use godot::{obj::WithBaseField, prelude::*};

use automerge::patches::TextRepresentation;
use automerge_repo::{tokio::FsStorage, ConnDirection, DocumentId, Repo, RepoHandle};
use tokio::{net::TcpStream, runtime::Runtime};
use std::ffi::c_void;
use std::os::raw::c_char;
use crate::godot_scene::{self, PackedGodotScene};

struct PatchWithScene {
    patch: Patch,
    scene: PackedGodotScene,
}
// #[godot_api]
// alas, We can't use Godot types because the rust godot library binds to the godot api.
// type AutoMergeSignalCallback = extern "C" fn(*mut c_void, *const std::os::raw::c_char, *const Variant, usize) -> ();
type AutoMergeSignalCallback = extern "C" fn(*mut c_void, *const std::os::raw::c_char, *const *const std::os::raw::c_char, usize) -> ();

// #[derive(GodotClass)]
// #[class(no_init, base=Node)]
pub struct AutomergeFS {
    repo_handle: RepoHandle,
    runtime: Runtime,
    fs_doc_id: DocumentId,
    signal_user_data: *mut c_void,
    signal_callback: AutoMergeSignalCallback,
    sender: Sender<PatchWithScene>,
    receiver: Receiver<PatchWithScene>,
}

const SERVER_URL: &str = "localhost:8080"; //"godot-rust.onrender.com:80";

static SIGNAL_FILE_CHANGED: std::sync::LazyLock<std::ffi::CString> = std::sync::LazyLock::new(|| std::ffi::CString::new("file_changed").unwrap());
static SIGNAL_STARTED: std::sync::LazyLock<std::ffi::CString> = std::sync::LazyLock::new(|| std::ffi::CString::new("started").unwrap());

// convert a slice of strings to a slice of char * strings (e.g. *const std::os::raw::c_char)
fn to_c_strs(strings: &[&str]) -> Vec<std::ffi::CString> {
    strings.iter().map(|s| std::ffi::CString::new(*s).unwrap()).collect()
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


impl AutomergeFS {
    // static signal char * file_changed = "file_changed";
    // #[signal]
    // fn file_changed(path: String, content: String);

    // #[func]
    fn get_fs_doc_id(&self) -> String {
        self.fs_doc_id.to_string()
    }

    
    // #[func]
    // hack: pass in empty string to create a new doc
    // godot rust doens't seem to support Option args
    fn create(maybe_fs_doc_id: String, signal_user_data: *mut c_void, signal_callback: AutoMergeSignalCallback) -> AutomergeFS {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = runtime.enter();

        let _ = tracing_subscriber::fmt::try_init();

        let storage = FsStorage::open("/tmp/automerge-godot-data").unwrap();
        let repo = Repo::new(None, Box::new(storage));
        let repo_handle = repo.run();
        let fs_doc_id = if maybe_fs_doc_id.is_empty() {
            let handle = repo_handle.new_document();
            handle.document_id()
        } else {
            DocumentId::from_str(&maybe_fs_doc_id).unwrap()
        };

        // connect repo
        let repo_handle_clone = repo_handle.clone();
        runtime.spawn(async move {
            println!("start a client");
            let mut refused = false;
            // Start a client.
            let stream = loop {
                // Try to connect to a peer
                let res = TcpStream::connect(SERVER_URL).await;
                if let Err(e) = res {
                    if e.kind() == std::io::ErrorKind::ConnectionRefused {
                        if !refused {
                            println!("connection refused, retrying...");
                            refused = true;
                        }
                    } else {
                        println!("error connecting: {:?}", e);
                    }
                    continue;
                }
                break res.unwrap();
            };

            println!("connect repo");

            repo_handle_clone
                .connect_tokio_io(SERVER_URL, stream, ConnDirection::Outgoing)
                .await
                .unwrap();
        });

        let (sender, receiver) = channel::<PatchWithScene>();

        AutomergeFS {
            repo_handle,
            fs_doc_id,
            runtime,
            signal_user_data,
            signal_callback,
            sender,
            receiver,
        }
    }

    // #[func]
    fn stop(&self) {
        self.repo_handle.clone().stop().unwrap();

        // todo: shut down runtime
        //self.runtime.shutdown_background();
    }

    // needs to be called in godot on each frame
    // #[func]
    fn refresh(&mut self) {
        
        // Collect all available updates
        let mut updates = Vec::new();
        while let Ok(update) = self.receiver.try_recv() {
            updates.push(update);
        }

        // Process all updates
        for patch_with_scene in updates {
            let PatchWithScene { patch, scene } = patch_with_scene;
            match patch.action {
                // handle update node
                automerge::PatchAction::PutMap {
                    key,
                    value,
                    conflict: _,
                } => match (patch.path.get(0), patch.path.get(1), patch.path.get(2)) {
                    (
                        Some((_, automerge::Prop::Map(maybe_nodes))),
                        Some((_, automerge::Prop::Map(node_path))),
                        Some((_, automerge::Prop::Map(prop_or_attr))),
                    ) => {
                        if maybe_nodes == "nodes" {
                            if let automerge::Value::Scalar(v) = value.0 {
                                if let ScalarValue::Str(smol_str) = v.as_ref() {
                                    let string_value = smol_str.to_string();

                                    // let mut dict = dict! {
                                    //     "file_path": "res://main.tscn",
                                    //     "node_path": node_path.to_variant(),
                                    //     "type": if prop_or_attr == "properties" {
                                    //         "property_changed"
                                    //     } else {
                                    //         "attribute_changed"
                                    //     },
                                    //     "key": key,
                                    //     "value": string_value,
                                    // };

                                    let mut dict: HashMap<&str, String> = HashMap::new();
                                    dict.insert("file_path", String::from("res://main.tscn"));
                                    dict.insert("node_path", node_path.clone());
                                    dict.insert("type", if prop_or_attr == "properties" {
                                        "property_changed".parse().unwrap()
                                    } else {
                                        "attribute_changed".parse().unwrap()
                                    });
                                    dict.insert("key", key);
                                    dict.insert("value", string_value);


                                    // Look up node in scene and get instance / type attribute if it exists
                                    if let Some(node) =
                                        godot_scene::get_node_by_path(&scene, node_path)
                                    {
                                        let attributes = godot_scene::get_node_attributes(&node);
                                        {
                                            if let Some(instance) = attributes.get("instance") {
                                                let _ = dict.insert("instance_path", instance.clone());
                                            } else if let Some(type_val) = attributes.get("type") {
                                                let _ = dict.insert("instance_type", type_val.clone());
                                            }
                                        }
                                    }
                                    // self.base_mut()
                                    //     .emit_signal("file_changed", &[dict.to_variant()]);
                                    // convert args to a slice
                                    let cs_args = to_c_strs_from_dict(&dict);
                                    let args = to_char_stars(&cs_args);
                                    (self.signal_callback)(self.signal_user_data, SIGNAL_FILE_CHANGED.as_ptr(), args.as_ptr(), args.len());

                                }
                            }
                        }
                    }
                    _ => {}
                },

                // handle delete node
                automerge::PatchAction::DeleteMap { key: node_path } => {
                    if patch.path.len() != 1 {
                        continue;
                    }
                    match patch.path.get(0) {
                        Some((_, automerge::Prop::Map(key))) => {
                            if key == "nodes" {
                                // self.base_mut().emit_signal(
                                //     "file_changed",
                                //     &[dict! {
                                //       "file_path": "res://main.tscn",
                                //       "node_path": node_path.to_variant(),
                                //       "type": "node_deleted",
                                //     }
                                let args = ["file_path", "res://main.tscn",
                                    "node_path", node_path.as_str(),
                                    "type", "node_deleted",
                                ];
                                let cs_args = to_c_strs(&args);
                                let args = to_char_stars(&cs_args);
                                (self.signal_callback)(
                                    self.signal_user_data,
                                    SIGNAL_FILE_CHANGED.as_ptr(),
                                    args.as_ptr(),
                                    args.len()
                                );
                            }
                        }
                        _ => {}
                    };
                }
                _ => {}
            }
        }
    }

    // #[func]
    fn start(&self) {
        // listen for changes to fs doc
        let repo_handle_change_listener = self.repo_handle.clone();
        let fs_doc_id = self.fs_doc_id.clone();
        let sender = self.sender.clone();
        self.runtime.spawn(async move {
            let mut doc_handle_result;
            loop {
                let cloned_fs_doc_id = fs_doc_id.clone();
                doc_handle_result = repo_handle_change_listener
                    .request_document(cloned_fs_doc_id)
                    .await
                ;
                if doc_handle_result.is_err() {
                    if let Some(e) = doc_handle_result.err() {
                        match e {
                            automerge_repo::RepoError::Shutdown => {
                                println!("RepoError::Shutdown");
                                return;
                            }
                            _ => {
                                println!("Error: {:?}", e);
                            }
                        }
                    }
                    continue;
                }
                break;
            }
            let doc_handle = doc_handle_result.unwrap();
            let mut heads: Vec<ChangeHash> = vec![];

            loop {
                doc_handle.changed().await.unwrap();

                doc_handle.with_doc(|d| -> () {
                    let new_heads = d.get_heads();
                    let patches = d.diff(&heads, &new_heads, TextRepresentation::String);
                    heads = new_heads;

                    // Hydrate the current document state into a PackedGodotScene
                    let scene: PackedGodotScene = hydrate(d).unwrap();

                    for patch in patches {
                        let patch_with_scene = PatchWithScene {
                            patch,
                            scene: scene.clone(),
                        };
                        let _ = sender.send(patch_with_scene);
                    }
                });
            }
        });
    }

    // #[func]
    fn save(&self, path: String, content: String) {
        let repo_handle = self.repo_handle.clone();
        let fs_doc_id = self.fs_doc_id.clone();

        // todo: handle files that are not main.tscn
        if !path.ends_with("main.tscn") {
            return;
        }

        let scene = godot_scene::parse(&content).unwrap();

        self.runtime.spawn(async move {
            let doc_handle = repo_handle.request_document(fs_doc_id);
            let result = doc_handle.await.unwrap();

            result.with_doc_mut(|d| {
                let mut tx = d.transaction();
                let _ = reconcile(&mut tx, scene);
                tx.commit();
                return;
            });
        });
    }
}


// C FFI functions for AutomergeFS

#[no_mangle]
pub extern "C" fn automerge_fs_get_fs_doc_id(automerge_fs: *const AutomergeFS) -> *const std::os::raw::c_char { 
    let automerge_fs = unsafe { &*automerge_fs };
    let fs_doc_id = automerge_fs.get_fs_doc_id();
    let c_string = std::ffi::CString::new(fs_doc_id).unwrap();
    c_string.into_raw()
}

// free const char * string; rust docs explicitly say you shouldn't attempt to call stdlib's free on a rust-allocated string
#[no_mangle]
pub extern "C" fn automerge_fs_free_string(s: *const std::os::raw::c_char) {
    unsafe {
        if s.is_null() {
            return;
        }
        drop(std::ffi::CString::from_raw(s as *mut c_char));
    }
}

#[no_mangle]
pub extern "C" fn automerge_fs_create(
    maybe_fs_doc_id: *const std::os::raw::c_char,
    signal_user_data: *mut c_void,
    signal_callback: AutoMergeSignalCallback,
) -> *mut AutomergeFS {
    let maybe_fs_doc_id = unsafe { std::ffi::CStr::from_ptr(maybe_fs_doc_id) }
        .to_str()
        .unwrap()
        .to_string();
    let automerge_fs = AutomergeFS::create(maybe_fs_doc_id, signal_user_data, signal_callback);
    Box::into_raw(Box::new(automerge_fs))
}

#[no_mangle]
pub extern "C" fn automerge_fs_stop(automerge_fs: *mut AutomergeFS) {
    let automerge_fs = unsafe { &mut *automerge_fs };
    automerge_fs.stop();
}

#[no_mangle]
pub extern "C" fn automerge_fs_refresh(automerge_fs: *mut AutomergeFS) {
    let automerge_fs = unsafe { &mut *automerge_fs };
    automerge_fs.refresh();
}

#[no_mangle]
pub extern "C" fn automerge_fs_start(automerge_fs: *const AutomergeFS) {
    let automerge_fs = unsafe { &*automerge_fs };
    automerge_fs.start();
}

#[no_mangle]
pub extern "C" fn automerge_fs_save(
    automerge_fs: *const AutomergeFS, path: *const std::os::raw::c_char, content: *const std::os::raw::c_char, _content_len: usize) {
    let automerge_fs = unsafe { &*automerge_fs };
    let path = unsafe { std::ffi::CStr::from_ptr(path) }
        .to_str()
        .unwrap()
        .to_string();
    // use content_len
    let content = unsafe { std::ffi::CStr::from_ptr(content) }
        .to_str()
        .unwrap()
        .to_string();
    automerge_fs.save(path, content);
}

#[no_mangle]
pub extern "C" fn automerge_fs_destroy(automerge_fs: *mut AutomergeFS) {
    unsafe {
        drop(Box::from_raw(automerge_fs));
    }
}
