#include "godot_project.h"

void GodotProjectWrapper::_signal_callback(void *signal_user_data, const char *signal, const char *const *args, size_t args_len) {
	Dictionary dictionary;
	ERR_FAIL_COND_MSG(args_len % 2 != 0, "Expected an even number of arguments");
	for (size_t i = 0; i < args_len; i += 2) {
		dictionary[String::utf8(args[i])] = String::utf8(args[i + 1]);
	}
	auto self = static_cast<GodotProjectWrapper *>(signal_user_data);
	self->signal_callback(signal, dictionary);
}

void GodotProjectWrapper::signal_callback(const String &signal, Dictionary args) {
	if (String(signal) == "file_changed") {
		emit_signal(SNAME("file_changed"), args);
	} else if (String(signal) == "started") {
		emit_signal(SNAME("started"));
	} else {
		ERR_FAIL_MSG("Unknown signal: " + String(signal));
	}
}

GodotProjectWrapper *GodotProjectWrapper::instance_and_create(const String &maybe_fs_doc_id) {
	GodotProjectWrapper *self{ memnew(GodotProjectWrapper) };
	self->create(maybe_fs_doc_id);
	return self;
}

GodotProjectWrapper::GodotProjectWrapper() :
		fs(nullptr) {
}

void GodotProjectWrapper::create(const String &p_maybe_fs_doc_id) {
	ERR_FAIL_COND_MSG(fs != nullptr, "AutomergeFS already created");
	fs = godot_project_create(p_maybe_fs_doc_id.utf8().get_data(), this, &_signal_callback);
	print_line("AutomergeFS created: " + String::num_int64((int64_t)fs));
}

GodotProjectWrapper::~GodotProjectWrapper() {
	if (fs != nullptr) {
		godot_project_stop(fs);
		godot_project_destroy(fs);
	}
}

void GodotProjectWrapper::process() {
	godot_project_process(fs);
}

void GodotProjectWrapper::stop() {
	godot_project_stop(fs);
}

void GodotProjectWrapper::save_file(const String &path, const String &content) {
	godot_project_save_file(fs, path.utf8().get_data(), content.utf8().get_data(), content.utf8().size());
}

String GodotProjectWrapper::get_fs_doc_id() const {
	auto id = godot_project_get_fs_doc_id(fs);
	auto strid = String(id);
	godot_project_free_string(id);
	return strid;
}

TypedArray<Dictionary> GodotProjectWrapper::get_branches() {
	TypedArray<Dictionary> branches;
	uint64_t len;
	auto branch_ids = godot_project_get_branches(fs, &len);
	for (uint64_t i = 0; i < len * 4; i += 4) {
		Dictionary branch;
		branch[String::utf8(branch_ids[i])] = String::utf8(branch_ids[i + 1]);
		branch[String::utf8(branch_ids[i + 2])] = String::utf8(branch_ids[i + 3]);
		branches.push_back(branch);
	}
	return branches;
}

void GodotProjectWrapper::checkout_branch(const String &branch_id) {
	godot_project_checkout_branch(fs, branch_id.utf8().get_data());
}

String GodotProjectWrapper::create_branch(const String &name) {
	return String::utf8(godot_project_create_branch(fs, name.utf8().get_data()));
}

String GodotProjectWrapper::get_checked_out_branch_id() const {
	return String::utf8(godot_project_get_checked_out_branch_id(fs));
}

void GodotProjectWrapper::_notification(int p_what) {
	switch (p_what) {
		case NOTIFICATION_PROCESS: {
			process();
		} break;
	}
}

void GodotProjectWrapper::_bind_methods() {
	ClassDB::bind_method(D_METHOD("refresh"), &GodotProjectWrapper::process);
	ClassDB::bind_method(D_METHOD("stop"), &GodotProjectWrapper::stop);
	ClassDB::bind_method(D_METHOD("save", "path", "content"), &GodotProjectWrapper::save_file);
	ADD_SIGNAL(MethodInfo("file_changed", PropertyInfo(Variant::DICTIONARY, "file")));
	ADD_SIGNAL(MethodInfo("started"));
}