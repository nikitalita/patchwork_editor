#include "patchwork_editor.h"

#include "missing_resource_container.h"

#include <core/io/json.h>
#include <core/io/missing_resource.h>
#include <core/variant/variant.h>
#include <editor/editor_file_system.h>
#include <editor/editor_inspector.h>
#include <editor/editor_undo_redo_manager.h>
#include <scene/resources/packed_scene.h>

PatchworkEditor::PatchworkEditor() {
}

PatchworkEditor::~PatchworkEditor() {
}

PatchworkEditor *PatchworkEditor::get_singleton() {
	return singleton;
}

void PatchworkEditor::_on_filesystem_changed() {
}

void PatchworkEditor::_on_resources_reloaded() {
}

void PatchworkEditor::_on_history_changed() {
	// // get the current edited scene
	// auto scene = EditorNode::get_singleton()->get_edited_scene();
	// if (scene == nullptr) {
	// 	return;
	// }
	// // pack the scene into a packed scene
	// auto packed_scene = memnew(PackedScene);
	// packed_scene->pack(scene);
	// // temp file name with random name
	// auto temp_file = "user://temp_" + itos(OS::get_singleton()->get_unix_time()) + ".tscn";
	// Error err = ResourceSaver::save(packed_scene, temp_file);
	// if (err != OK) {
	// 	print_line("Error saving scene");
	// 	return;
	// }
	// // open the file
	// auto file = FileAccess::open(temp_file, FileAccess::READ);
	// if (file.is_valid()) {
	// 	auto contents = file->get_as_text();
	// 	auto scene_path = scene->get_scene_file_path();
	// 	if (scene_path == "res://main.tscn") {
	// 		fs->save_file(scene->get_scene_file_path(), contents);
	// 		// test getting the file
	// 		// auto file_contents = fs->get_file(scene->get_scene_file_path());
	// 		// if (file_contents != contents) {
	// 		// 	print_line("File contents do not match");
	// 		// } else {
	// 		// 	print_line("Yay");
	// 		// }
	// 	}
	// 	file->close();
	// }
	// DirAccess::remove_absolute(temp_file);
}

void PatchworkEditor::handle_change(const String &resource_path, const NodePath &node_path, HashMap<String, Variant> properties) {
	// auto res = ResourceLoader::load(resource_path);
	// if (!node_path.is_empty()) {
	// 	Ref<PackedScene> scene = res;
	// 	auto node_idx = scene->get_state()->find_node_by_path(node_path);
	// }
}

void PatchworkEditor::_on_file_changed(Dictionary dict) {
	// let args = ["file_path", "res://main.tscn",
	// "node_path", node_path.as_str(),
	// "type", "node_deleted",
	// ];
	// auto file_path = dict["file_path"];
	// auto node_path = dict["node_path"];
}

void PatchworkEditor::_notification(int p_what) {
	switch (p_what) {
		case NOTIFICATION_READY: {
			print_line("Entered tree");
			break;
		}
		default:
			break;
	}
}

bool PatchworkEditor::unsaved_files_open() {
	auto opened_scenes = EditorNode::get_editor_data().get_edited_scenes();
	for (int i = 0; i < opened_scenes.size(); i++) {
		auto id = opened_scenes[i].history_id;
		if (EditorUndoRedoManager::get_singleton()->is_history_unsaved(id)) {
			return true;
		}
	}
	// Not bound
	if (EditorUndoRedoManager::get_singleton()->is_history_unsaved(EditorUndoRedoManager::GLOBAL_HISTORY)) {
		return true;
	}
	// do the same for scripts
	return false;
}

bool PatchworkEditor::detect_utf8(const PackedByteArray &p_utf8_buf) {
	int cstr_size = 0;
	int str_size = 0;
	const char *p_utf8 = (const char *)p_utf8_buf.ptr();
	int p_len = p_utf8_buf.size();
	if (p_len == 0) {
		return true; // empty string
	}
	bool p_skip_cr = false;
	/* HANDLE BOM (Byte Order Mark) */
	if (p_len < 0 || p_len >= 3) {
		bool has_bom = uint8_t(p_utf8[0]) == 0xef && uint8_t(p_utf8[1]) == 0xbb && uint8_t(p_utf8[2]) == 0xbf;
		if (has_bom) {
			//8-bit encoding, byte order has no meaning in UTF-8, just skip it
			if (p_len >= 0) {
				p_len -= 3;
			}
			p_utf8 += 3;
		}
	}

	// bool decode_error = false;
	// bool decode_failed = false;
	{
		const char *ptrtmp = p_utf8;
		const char *ptrtmp_limit = p_len >= 0 ? &p_utf8[p_len] : nullptr;
		int skip = 0;
		uint8_t c_start = 0;
		while (ptrtmp != ptrtmp_limit && *ptrtmp) {
#if CHAR_MIN == 0
			uint8_t c = *ptrtmp;
#else
			uint8_t c = *ptrtmp >= 0 ? *ptrtmp : uint8_t(256 + *ptrtmp);
#endif

			if (skip == 0) {
				if (p_skip_cr && c == '\r') {
					ptrtmp++;
					continue;
				}
				/* Determine the number of characters in sequence */
				if ((c & 0x80) == 0) {
					skip = 0;
				} else if ((c & 0xe0) == 0xc0) {
					skip = 1;
				} else if ((c & 0xf0) == 0xe0) {
					skip = 2;
				} else if ((c & 0xf8) == 0xf0) {
					skip = 3;
				} else if ((c & 0xfc) == 0xf8) {
					skip = 4;
				} else if ((c & 0xfe) == 0xfc) {
					skip = 5;
				} else {
					skip = 0;
					// print_unicode_error(vformat("Invalid UTF-8 leading byte (%x)", c), true);
					// decode_failed = true;
					return false;
				}
				c_start = c;

				if (skip == 1 && (c & 0x1e) == 0) {
					// print_unicode_error(vformat("Overlong encoding (%x ...)", c));
					// decode_error = true;
					return false;
				}
				str_size++;
			} else {
				if ((c_start == 0xe0 && skip == 2 && c < 0xa0) || (c_start == 0xf0 && skip == 3 && c < 0x90) || (c_start == 0xf8 && skip == 4 && c < 0x88) || (c_start == 0xfc && skip == 5 && c < 0x84)) {
					// print_unicode_error(vformat("Overlong encoding (%x %x ...)", c_start, c));
					// decode_error = true;
					return false;
				}
				if (c < 0x80 || c > 0xbf) {
					// print_unicode_error(vformat("Invalid UTF-8 continuation byte (%x ... %x ...)", c_start, c), true);
					// decode_failed = true;
					return false;

					// skip = 0;
				} else {
					--skip;
				}
			}

			cstr_size++;
			ptrtmp++;
		}
		// not checking for last sequence because we pass in incomplete bytes
		// if (skip) {
		// print_unicode_error(vformat("Missing %d UTF-8 continuation byte(s)", skip), true);
		// decode_failed = true;
		// return false;
		// }
	}

	if (str_size == 0) {
		// clear();
		return true; // empty string
	}

	// resize(str_size + 1);
	// char32_t *dst = ptrw();
	// dst[str_size] = 0;

	int skip = 0;
	uint32_t unichar = 0;
	while (cstr_size) {
#if CHAR_MIN == 0
		uint8_t c = *p_utf8;
#else
		uint8_t c = *p_utf8 >= 0 ? *p_utf8 : uint8_t(256 + *p_utf8);
#endif

		if (skip == 0) {
			if (p_skip_cr && c == '\r') {
				p_utf8++;
				continue;
			}
			/* Determine the number of characters in sequence */
			if ((c & 0x80) == 0) {
				// *(dst++) = c;
				unichar = 0;
				skip = 0;
			} else if ((c & 0xe0) == 0xc0) {
				unichar = (0xff >> 3) & c;
				skip = 1;
			} else if ((c & 0xf0) == 0xe0) {
				unichar = (0xff >> 4) & c;
				skip = 2;
			} else if ((c & 0xf8) == 0xf0) {
				unichar = (0xff >> 5) & c;
				skip = 3;
			} else if ((c & 0xfc) == 0xf8) {
				unichar = (0xff >> 6) & c;
				skip = 4;
			} else if ((c & 0xfe) == 0xfc) {
				unichar = (0xff >> 7) & c;
				skip = 5;
			} else {
				// *(dst++) = _replacement_char;
				// unichar = 0;
				// skip = 0;
				return false;
			}
		} else {
			if (c < 0x80 || c > 0xbf) {
				// *(dst++) = _replacement_char;
				skip = 0;
			} else {
				unichar = (unichar << 6) | (c & 0x3f);
				--skip;
				if (skip == 0) {
					if (unichar == 0) {
						return false;
						// print_unicode_error("NUL character", true);
						// decode_failed = true;
						// unichar = _replacement_char;
					} else if ((unichar & 0xfffff800) == 0xd800) {
						return false;

						// print_unicode_error(vformat("Unpaired surrogate (%x)", unichar), true);
						// decode_failed = true;
						// unichar = _replacement_char;
					} else if (unichar > 0x10ffff) {
						return false;

						// print_unicode_error(vformat("Invalid unicode codepoint (%x)", unichar), true);
						// decode_failed = true;
						// unichar = _replacement_char;
					}
					// *(dst++) = unichar;
				}
			}
		}

		cstr_size--;
		p_utf8++;
	}
	if (skip) {
		// return false;
		// *(dst++) = 0x20;
	}

	return true;
}

Vector<String> PatchworkEditor::get_recursive_dir_list(const String &p_dir, const Vector<String> &wildcards, const bool absolute, const String &rel) {
	Vector<String> ret;
	Error err;
	Ref<DirAccess> da = DirAccess::open(p_dir.path_join(rel), &err);
	ERR_FAIL_COND_V_MSG(da.is_null(), ret, "Failed to open directory " + p_dir);

	if (da.is_null()) {
		return ret;
	}
	Vector<String> dirs;
	Vector<String> files;

	String base = absolute ? p_dir : "";
	da->list_dir_begin();
	String f = da->get_next();
	while (!f.is_empty()) {
		if (f == "." || f == "..") {
			f = da->get_next();
			continue;
		} else if (da->current_is_dir()) {
			dirs.push_back(f);
		} else {
			files.push_back(f);
		}
		f = da->get_next();
	}
	da->list_dir_end();

	dirs.sort_custom<FileNoCaseComparator>();
	files.sort_custom<FileNoCaseComparator>();
	for (auto &d : dirs) {
		ret.append_array(get_recursive_dir_list(p_dir, wildcards, absolute, rel.path_join(d)));
	}
	for (auto &file : files) {
		if (wildcards.size() > 0) {
			for (int i = 0; i < wildcards.size(); i++) {
				if (file.get_file().matchn(wildcards[i])) {
					ret.append(base.path_join(rel).path_join(file));
					break;
				}
			}
		} else {
			ret.append(base.path_join(rel).path_join(file));
		}
	}

	return ret;
}

void PatchworkEditor::progress_add_task(const String &p_task, const String &p_label, int p_steps, bool p_can_cancel) {
	EditorNode::get_singleton()->progress_add_task(p_task, p_label, p_steps, p_can_cancel);
}

bool PatchworkEditor::progress_task_step(const String &p_task, const String &p_state, int p_step, bool p_force_refresh) {
	return EditorNode::get_singleton()->progress_task_step(p_task, p_state, p_step, p_force_refresh);
}

void PatchworkEditor::progress_end_task(const String &p_task) {
	EditorNode::get_singleton()->progress_end_task(p_task);
}
void PatchworkEditor::progress_add_task_bg(const String &p_task, const String &p_label, int p_steps) {
	EditorNode::get_singleton()->progress_add_task_bg(p_task, p_label, p_steps);
}
void PatchworkEditor::progress_task_step_bg(const String &p_task, int p_step) {
	EditorNode::get_singleton()->progress_task_step_bg(p_task, p_step);
}
void PatchworkEditor::progress_end_task_bg(const String &p_task) {
	EditorNode::get_singleton()->progress_end_task_bg(p_task);
}


Dictionary PatchworkEditor::get_diff(Dictionary changed_files_dict) {
	// files = [{
	// 	path: "path/to/file",
	// 	change: "modified",
	// 	old_content: "res://path/to/file_old",
	// 	new_content: "res://path/to/file_new"
	// }, ...]
	Array files = changed_files_dict["files"];
	Dictionary display_diff;
	for (const auto &d : files) {
		Dictionary dict = d;
		if (dict.size() == 0) {
			continue;
		}
		String change_type = dict["change"];
		String path = dict["path"];
		if (change_type == "modified") {
			auto old_content = dict["old_content"];
			auto new_content = dict["new_content"];
			auto diff = get_file_diff(old_content, new_content);

			display_diff[path] = diff;
			// show diff
		} else if (change_type == "added" || change_type == "deleted") {
			Dictionary diff;
			diff["type"] = change_type;
			display_diff[path] = diff;
		}
	}
	return display_diff;
}

Dictionary PatchworkEditor::get_file_diff(const String &p_path, const String &p_path2) {
	Error error = OK;
	auto res1 = ResourceLoader::load(p_path, "", ResourceFormatLoader::CACHE_MODE_IGNORE_DEEP, &error);
	ERR_FAIL_COND_V_MSG(error != OK, Dictionary(), "Failed to load resource at path " + p_path);
	auto res2 = ResourceLoader::load(p_path2, "", ResourceFormatLoader::CACHE_MODE_IGNORE_DEEP, &error);
	ERR_FAIL_COND_V_MSG(error != OK, Dictionary(), "Failed to load resource at path " + p_path2);
	return get_diff_res(res1, res2);
}

bool PatchworkEditor::deep_equals(Variant a, Variant b, bool exclude_non_storage) {
	if (a.get_type() != b.get_type()) {
		return false;
	}
	// we only check for Arrays, Objects, and Dicts; the rest have the overloaded == operator
	switch (a.get_type()) {
		case Variant::NIL: {
			return true;
		}
		case Variant::ARRAY: {
			Array arr_a = a;
			Array arr_b = b;
			if (arr_a.size() != arr_b.size()) {
				return false;
			}
			for (int i = 0; i < arr_a.size(); i++) {
				if (!deep_equals(arr_a[i], arr_b[i])) {
					return false;
				}
			}
			break;
		}
		case Variant::DICTIONARY: {
			Dictionary dict_a = a;
			Dictionary dict_b = b;
			if (dict_a.size() != dict_b.size()) {
				return false;
			}
			for (const Variant &key : dict_a.keys()) {
				if (!dict_b.has(key)) {
					return false;
				}
				if (!deep_equals(dict_a[key], dict_b[key])) {
					return false;
				}
			}
			break;
		}
		case Variant::OBJECT: {
			Object *obj_a = a;
			Object *obj_b = b;
			if (obj_a == obj_b) {
				return true;
			}
			if (obj_a == nullptr || obj_b == nullptr) {
				return false;
			}
			if (obj_a->get_class() != obj_b->get_class()) {
				return false;
			}
			List<PropertyInfo> p_list_a;
			List<PropertyInfo> p_list_b;
			obj_a->get_property_list(&p_list_a, false);
			obj_b->get_property_list(&p_list_b, false);
			if (p_list_a.size() != p_list_b.size()) {
				return false;
			}
			for (auto &prop : p_list_a) {
				if (exclude_non_storage && !(prop.usage & PROPERTY_USAGE_STORAGE)) {
					continue;
				}
				auto prop_name = prop.name;
				if (!deep_equals(obj_a->get(prop_name), obj_b->get(prop_name))) {
					return false;
				}
			}
			break;
		}
		default: {
			return a == b;
		}
	}
	return true;
}

Dictionary PatchworkEditor::get_diff_obj(Object *a, Object *b, bool exclude_non_storage) {
	Dictionary diff;
	List<PropertyInfo> p_list_a;
	List<PropertyInfo> p_list_b;
	a->get_property_list(&p_list_a, false);
	b->get_property_list(&p_list_b, false);
	// diff is key: [old_value, new_value]
	HashSet<String> prop_names;
	// TODO: handle PROPERTY_USAGE_NO_EDITOR, PROPERTY_USAGE_INTERNAL, etc.
	for (auto &prop : p_list_a) {
		if (exclude_non_storage && !(prop.usage & PROPERTY_USAGE_STORAGE)) {
			continue;
		}
		prop_names.insert(prop.name);
	}
	for (auto &prop : p_list_b) {
		if (exclude_non_storage && !(prop.usage & PROPERTY_USAGE_STORAGE)) {
			continue;
		}
		prop_names.insert(prop.name);
	}
	for (auto &prop : prop_names) {
		auto prop_a = a->get(prop);
		auto prop_b = b->get(prop);
		if (!deep_equals(prop_a, prop_b)) {
			auto arr = Array();
			arr.push_back(prop_a);
			arr.push_back(prop_b);
			diff[prop] = arr;
		}
	}
	return diff;
}

void get_child_node_paths(Node *node_a, HashSet<NodePath> &paths, const String &curr_path = ".") {
	for (int i = 0; i < node_a->get_child_count(); i++) {
		auto child_a = node_a->get_child(i);
		auto new_path = curr_path.path_join(child_a->get_name());
		paths.insert(new_path);
		get_child_node_paths(child_a, paths, new_path);
	}
}

Dictionary PatchworkEditor::evaluate_node_differences(Node *scene1, Node *scene2, const NodePath &path) {
	Dictionary result;
	bool is_root = path == "." || path.is_empty();
	Node *node1 = scene1;
	Node *node2 = scene2;
	if (!is_root) {
		if (node1->has_node(path)) {
			node1 = node1->get_node(path);
		} else {
			node1 = nullptr;
		}
		if (node2->has_node(path)) {
			node2 = node2->get_node(path);
		} else {
			node2 = nullptr;
		}
		result["path"] = path;
	} else {
		result["path"] = "." + scene1->get_name();
	}
	if (node1 == nullptr) {
		result["type"] = "node_added";
		return result;
	}
	if (node2 == nullptr) {
		result["type"] = "node_deleted";
		return result;
	}
	auto diff = get_diff_obj(node1, node2, true);
	if (diff.size() > 0) {
		result["type"] = "node_changed";
		// TODO: handle scene changes
		result["props"] = diff;
		return result;
	}
	return Dictionary();
}

Dictionary PatchworkEditor::get_diff_res(Ref<Resource> p_res, Ref<Resource> p_res2) {
	// check the types
	Dictionary result;
	if (p_res->get_class() != p_res2->get_class()) {
		result["type"] = "type_changed";
		result["old_type"] = p_res->get_class();
		result["new_type"] = p_res2->get_class();
		return result;
	}
	// ensure that the references stick around
	result["res_old"] = p_res;
	result["res_new"] = p_res2;
	if (p_res->get_class() != "PackedScene") {
		result["type"] = "resource_changed";
		result["props"] = get_diff_obj(p_res.ptr(), p_res2.ptr(), true);
		return result;
	}
	// otherwise, we have to instantiate the scenes and compare them
	Ref<PackedScene> p_scene1 = p_res;
	Ref<PackedScene> p_scene2 = p_res2;
	auto scene1 = p_scene1->instantiate();
	auto scene2 = p_scene2->instantiate();
	HashSet<NodePath> paths;
	get_child_node_paths(scene1, paths);
	get_child_node_paths(scene2, paths);
	Array node_diffs;
	for (auto &path : paths) {
		Dictionary value1 = (evaluate_node_differences(scene1, scene2, path));
		if (value1.size() > 0) {
			value1["res_old"] = p_res;
			value1["res_new"] = p_res2;
			node_diffs.push_back(value1);
		}
	}
	result["type"] = "scene_changed";
	result["nodes"] = node_diffs;
	return result;
}

PatchworkEditor *PatchworkEditor::singleton = nullptr;

PatchworkEditor::PatchworkEditor(EditorNode *p_editor) {
	singleton = this;
	editor = p_editor;
	// EditorUndoRedoManager::get_singleton()->connect(SNAME("history_changed"), callable_mp(this, &PatchworkEditor::_on_history_changed));
	//
	// fs = GodotProject::create("");
	// this->add_child(fs);
	// EditorFileSystem::get_singleton()->connect("filesystem_changed", callable_mp(this, &PatchworkEditor::signal_callback));
}

void PatchworkEditor::_bind_methods() {
	ClassDB::bind_static_method(get_class_static(), D_METHOD("progress_add_task", "task", "label", "steps", "can_cancel"), &PatchworkEditor::progress_add_task);
	ClassDB::bind_static_method(get_class_static(), D_METHOD("progress_task_step", "task", "state", "step", "force_refresh"), &PatchworkEditor::progress_task_step);
	ClassDB::bind_static_method(get_class_static(), D_METHOD("progress_end_task", "task"), &PatchworkEditor::progress_end_task);

	ClassDB::bind_static_method(get_class_static(), D_METHOD("progress_add_task_bg", "task", "label", "steps"), &PatchworkEditor::progress_add_task_bg);
	ClassDB::bind_static_method(get_class_static(), D_METHOD("progress_task_step_bg", "task", "step"), &PatchworkEditor::progress_task_step_bg);
	ClassDB::bind_static_method(get_class_static(), D_METHOD("progress_end_task_bg", "task"), &PatchworkEditor::progress_end_task_bg);

	ClassDB::bind_static_method(get_class_static(), "unsaved_files_open", &PatchworkEditor::unsaved_files_open);
	ClassDB::bind_static_method(get_class_static(), D_METHOD("detect_utf8", "utf8_buf"), &PatchworkEditor::detect_utf8);
	ClassDB::bind_static_method(get_class_static(), D_METHOD("get_recursive_dir_list", "dir", "wildcards", "absolute", "rel"), &PatchworkEditor::get_recursive_dir_list);
	ClassDB::bind_static_method(get_class_static(), D_METHOD("get_diff", "changed_files_dict"), &PatchworkEditor::get_diff);

	ClassDB::bind_static_method(get_class_static(), D_METHOD("get_file_diff", "old_path", "new_path"), &PatchworkEditor::get_file_diff);
	ClassDB::bind_static_method(get_class_static(), D_METHOD("deep_equals", "a", "b", "exclude_non_storage"), &PatchworkEditor::deep_equals, DEFVAL(true));
	ClassDB::bind_static_method(get_class_static(), D_METHOD("get_diff_obj", "a", "b", "exclude_non_storage"), &PatchworkEditor::get_diff_obj, DEFVAL(true));
	ClassDB::bind_static_method(get_class_static(), D_METHOD("evaluate_node_differences", "scene1", "scene2", "path"), &PatchworkEditor::evaluate_node_differences);
	ClassDB::bind_static_method(get_class_static(), D_METHOD("get_diff_res", "res1", "res2"), &PatchworkEditor::get_diff_res);
}
