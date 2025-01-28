#include "godot_project.h"
#include "core/object/object.h"

#include <editor/editor_node.h>
#include <editor/editor_undo_redo_manager.h>

void GodotProject::_signal_callback(void *signal_user_data, const char *signal, const char *const *p_args, size_t p_args_len) {
	Vector<String> args;
	ERR_FAIL_COND_MSG(p_args_len % 2 != 0, "Expected an even number of arguments");
	for (size_t i = 0; i < p_args_len; i++) {
		args.push_back(p_args[i]);
	}
	auto self = static_cast<GodotProject *>(signal_user_data);
	self->signal_callback(signal, args);
}

void GodotProject::signal_callback(const String &signal, const Vector<String> &args) {
	if (String(signal) == "files_changed") {
		emit_signal(SNAME("files_changed"));
	} else if (String(signal) == "checked_out_branch") {
		emit_signal(SNAME("checked_out_branch") /*, args[0]*/);
	} else if (String(signal) == "branches_changed") {
		emit_signal(SNAME("branches_changed"));
	} else {
		ERR_FAIL_MSG("Unknown signal: " + String(signal));
	}
}
bool GodotProject::unsaved_files_open() const {
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

GodotProject *GodotProject::create(const String &maybe_fs_doc_id) {
	GodotProject *self{ memnew(GodotProject) };
	self->init(maybe_fs_doc_id);
	return self;
}

GodotProject::GodotProject() :
		fs(nullptr) {
}

void GodotProject::init(const String &p_maybe_fs_doc_id) {
	ERR_FAIL_COND_MSG(fs != nullptr, "godot project already created");
	fs = godot_project_create(p_maybe_fs_doc_id.utf8().get_data(), this, &_signal_callback);
	print_line("godot project created: " + String::num_int64((int64_t)fs));
}

GodotProject::~GodotProject() {
	if (fs != nullptr) {
		godot_project_stop(fs);
		godot_project_destroy(fs);
	}
}

void GodotProject::process() {
	godot_project_process(fs);
}

void GodotProject::stop() {
	godot_project_stop(fs);
}

Error GodotProject::save_file(const String &path, const Variant &content) {
	// TODO: make godot_project_save_file return an error code
	if (content.get_type() == Variant::STRING) {
		auto content_str = content.operator String().utf8();
		godot_project_save_file(fs, path.utf8().get_data(), content_str.get_data(), content_str.size(), false);
	} else if (content.get_type() == Variant::PACKED_BYTE_ARRAY) {
		godot_project_save_file(fs, path.utf8().get_data(), (const char *)content.operator PackedByteArray().ptr(), content.operator PackedByteArray().size(), true);
	} else {
		ERR_FAIL_V_MSG(ERR_INVALID_PARAMETER, "Unsupported content type: " + String::num_int64(content.get_type()));
	}
	return OK;
}

Error GodotProject::save_file_at(const String &path, const Variant &heads, const Variant &content) {
	print_line("save " + String(heads));

	return OK;
}

Variant GodotProject::get_file(const String &path) {
	uint8_t is_binary;
	uint64_t length;
	Variant variant;
	auto buf_ptr = godot_project_get_file(fs, path.utf8().get_data(), &length, &is_binary);
	if (is_binary) {
		auto arr = PackedByteArray();
		arr.resize(length);
		for (uint64_t i = 0; i < length; i++) {
			arr.set(i, buf_ptr[i]);
		}
		variant = arr;
	} else {
		auto str = String::utf8(buf_ptr, length);
		variant = str;
	}	
	godot_project_free_string(buf_ptr);
	return variant;
}

String GodotProject::get_doc_id() const {
	auto id = godot_project_get_fs_doc_id(fs);
	auto strid = String(id);
	godot_project_free_string(id);
	return strid;
}

String GodotProject::get_branch_doc_id() const {
	auto id = godot_project_get_branch_doc_id(fs);
	auto strid = String::utf8(id);
	godot_project_free_string(id);
	return strid;
}

TypedArray<Dictionary> GodotProject::get_branches() {
	TypedArray<Dictionary> branches;
	uint64_t len;
	auto branch_ids = godot_project_get_branches(fs, &len);
	for (uint64_t i = 0; i < len * 4; i += 4) {
		Dictionary branch;
		branch[String::utf8(branch_ids[i])] = String::utf8(branch_ids[i + 1]);
		branch[String::utf8(branch_ids[i + 2])] = String::utf8(branch_ids[i + 3]);
		branches.push_back(branch);
	}
	godot_project_free_vec_string(branch_ids, len * 4);

	return branches;
}

void GodotProject::checkout_branch(const String &branch_id) {
	godot_project_checkout_branch(fs, branch_id.utf8().get_data());
}

String GodotProject::create_branch(const String &name) {
	// return String::utf8(godot_project_create_branch(fs, name.utf8().get_data()));
	auto rust_str = godot_project_create_branch(fs, name.utf8().get_data());
	auto str = String::utf8(rust_str);
	godot_project_free_string(rust_str);
	return str;
}

void GodotProject::merge_branch(const String &branch_id) {
	godot_project_merge_branch(fs, branch_id.utf8().get_data());
}

String GodotProject::get_checked_out_branch_id() const {
	auto rust_str = godot_project_get_checked_out_branch_id(fs);
	auto str = String::utf8(rust_str);
	godot_project_free_string(rust_str);
	return str;
}

//godot_project_list_all_files
Vector<String> GodotProject::list_all_files() {
	Vector<String> files;
	uint64_t len;
	auto branch_files = godot_project_list_all_files(fs, &len);
	for (uint64_t i = 0; i < len; i++) {
		files.push_back(String::utf8(branch_files[i]));
	}
	godot_project_free_vec_string(branch_files, len);
	return files;
}

//godot_project_get_heads
Vector<String> GodotProject::get_heads() {
	Vector<String> heads;
	uint64_t len;
	auto branch_heads = godot_project_get_heads(fs, &len);
	for (uint64_t i = 0; i < len; i++) {
		heads.push_back(String::utf8(branch_heads[i]));
	}
	godot_project_free_vec_string(branch_heads, len);
	return heads;
}

Vector<String> GodotProject::get_changes() {
	Vector<String> changes;
	uint64_t len;

	auto change_list = godot_project_get_changes(fs, &len);
	for (uint64_t i = 0; i < len; i++) {
		changes.push_back(String::utf8(change_list[i]));
	}
	godot_project_free_vec_string(change_list, len);
	return changes;
}

void GodotProject::_notification(int p_what) {
	switch (p_what) {
		case NOTIFICATION_PROCESS: {
			process();
		} break;
	}
}

// State sync functions 

int64_t GodotProject::get_state_int(const String &entity_id, const String &prop) {
CharString entity_id_utf8 = entity_id.utf8();
    CharString prop_utf8 = prop.utf8();
    
    const char *entity_id_cstr = entity_id_utf8.get_data();
    const char *prop_cstr = prop_utf8.get_data();

    const int64_t *value = godot_project_get_state_int(fs, entity_id_cstr, prop_cstr);

    if (value == nullptr) {
        return 0; // todo: can we return null?
    }
    int64_t result = *value;

		// todo: do we need to free value?

    return result;
}

void GodotProject::set_state_int(const String &entity_id, const String &prop, int64_t value) {
	// Keep the CharString objects alive for the duration of the function
	CharString entity_id_utf8 = entity_id.utf8();
	CharString prop_utf8 = prop.utf8();

	const char *entity_id_cstr = entity_id_utf8.get_data();
	const char *prop_cstr = prop_utf8.get_data();

	godot_project_set_state_int(fs, entity_id_cstr, prop_cstr, value);
}


void GodotProject::_bind_methods() {
	ClassDB::bind_method(D_METHOD("refresh"), &GodotProject::process);
	ClassDB::bind_method(D_METHOD("stop"), &GodotProject::stop);
	ClassDB::bind_method(D_METHOD("save_file", "path", "content"), &GodotProject::save_file);
	ClassDB::bind_method(D_METHOD("save_file_at", "path", "heads", "content"), &GodotProject::save_file_at);
	ClassDB::bind_method(D_METHOD("get_file", "path"), &GodotProject::get_file);
	ClassDB::bind_method(D_METHOD("get_doc_id"), &GodotProject::get_doc_id);
	ClassDB::bind_method(D_METHOD("get_branches"), &GodotProject::get_branches);
	ClassDB::bind_method(D_METHOD("checkout_branch", "branch_id"), &GodotProject::checkout_branch);
	ClassDB::bind_method(D_METHOD("merge_branch", "branch_id"), &GodotProject::merge_branch);
	ClassDB::bind_method(D_METHOD("create_branch", "name"), &GodotProject::create_branch);
	ClassDB::bind_method(D_METHOD("get_checked_out_branch_id"), &GodotProject::get_checked_out_branch_id);
	ClassDB::bind_method(D_METHOD("list_all_files"), &GodotProject::list_all_files);
	ClassDB::bind_method(D_METHOD("get_heads"), &GodotProject::get_heads);
	ClassDB::bind_method(D_METHOD("get_changes"), &GodotProject::get_changes);
	ClassDB::bind_method(D_METHOD("process"), &GodotProject::process);
	ClassDB::bind_method(D_METHOD("get_branch_doc_id"), &GodotProject::get_branch_doc_id);
	ClassDB::bind_method(D_METHOD("get_state_int", "entity_id", "prop"), &GodotProject::get_state_int);
	ClassDB::bind_method(D_METHOD("set_state_int", "entity_id", "prop", "value"), &GodotProject::set_state_int);
	//unsaved_files_open()
	ClassDB::bind_method(D_METHOD("unsaved_files_open"), &GodotProject::unsaved_files_open);
	ClassDB::bind_static_method(get_class_static(), SNAME("create"), &GodotProject::create);
	ADD_SIGNAL(MethodInfo("files_changed"));
	ADD_SIGNAL(MethodInfo("branches_changed"));
	ADD_SIGNAL(MethodInfo("checked_out_branch" /*,PropertyInfo(Variant::STRING, "branch_id")*/));
	// ADD_SIGNAL(MethodInfo("started"));
}