#include "automerge_fs.h"

void AutomergeFSWrapper::_signal_callback(void *signal_user_data, const char *signal, const char*const * args, size_t args_len) {
	Dictionary dictionary;
	ERR_FAIL_COND_MSG(args_len % 2 != 0, "Expected an even number of arguments");
	for (size_t i = 0; i < args_len; i += 2) {
		dictionary[String::utf8(args[i])] = String::utf8(args[i + 1]);
	}
	auto self = static_cast<AutomergeFSWrapper *>(signal_user_data);
	self->signal_callback(signal, dictionary);
}

void AutomergeFSWrapper::signal_callback(const String &signal, Dictionary args){
	if (String(signal) == "file_changed") {
		emit_signal(SNAME("file_changed"), args);
	} else if (String(signal) == "started") {
		emit_signal(SNAME("started"));
	} else {
		ERR_FAIL_MSG("Unknown signal: " + String(signal));
	}
}

AutomergeFSWrapper *AutomergeFSWrapper::instance_and_create(const String &maybe_fs_doc_id) {
	AutomergeFSWrapper *self{ memnew(AutomergeFSWrapper) };
	self->create(maybe_fs_doc_id);
	return self;
}

AutomergeFSWrapper::AutomergeFSWrapper() :
		fs(nullptr) {
}

void AutomergeFSWrapper::create(const String &p_maybe_fs_doc_id) {
	ERR_FAIL_COND_MSG(fs != nullptr, "AutomergeFS already created");
	fs = automerge_fs_create(p_maybe_fs_doc_id.utf8().get_data(), this, &_signal_callback);
	print_line("AutomergeFS created: " + String::num_int64((int64_t)fs));
}

AutomergeFSWrapper::~AutomergeFSWrapper() {
	automerge_fs_stop(fs);
	automerge_fs_destroy(fs);
}

void AutomergeFSWrapper::refresh() {
	automerge_fs_refresh(fs);
}

void AutomergeFSWrapper::start() {
	print_line("Starting....");
	automerge_fs_start(fs);
}

void AutomergeFSWrapper::stop() {
	automerge_fs_stop(fs);
}

void AutomergeFSWrapper::save(const String &path, const String &content) {
	automerge_fs_save(fs, path.utf8().get_data(), content.utf8().get_data(), content.utf8().size());
}

String AutomergeFSWrapper::get_fs_doc_id() const {
	auto id = automerge_fs_get_fs_doc_id(fs);
	auto strid = String(id);
	automerge_fs_free_string(id);
	return strid;
}

void AutomergeFSWrapper::_notification(int p_what) {
	switch (p_what) {
		case NOTIFICATION_PROCESS: {
			refresh();
		} break;
	}
}

void AutomergeFSWrapper::_bind_methods() {
	ClassDB::bind_method(D_METHOD("refresh"), &AutomergeFSWrapper::refresh);
	ClassDB::bind_method(D_METHOD("start"), &AutomergeFSWrapper::start);
	ClassDB::bind_method(D_METHOD("stop"), &AutomergeFSWrapper::stop);
	ClassDB::bind_method(D_METHOD("save", "path", "content"), &AutomergeFSWrapper::save);
	ADD_SIGNAL(MethodInfo("file_changed", PropertyInfo(Variant::DICTIONARY, "file")));
	ADD_SIGNAL(MethodInfo("started"));
}