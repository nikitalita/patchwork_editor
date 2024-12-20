#pragma once
#include "core/string/print_string.h"
#include "core/variant/variant.h"
#include "patchwork_rust/patchwork_rust.h"
#include "scene/main/node.h"

class AutomergeFSWrapper : public Node {
	GDCLASS(AutomergeFSWrapper, Node);

	static void _signal_callback(void *signal_user_data, const char *signal, const char* const * args, size_t args_len);
	void signal_callback(const String &signal, Dictionary args);

public:
	static AutomergeFSWrapper *instance_and_create(const String &maybe_fs_doc_id);
	AutomergeFSWrapper();
	~AutomergeFSWrapper();

	void create(const String &maybe_fs_doc_id);

	void refresh();

	void start();

	void stop();

	void save(const String &path, const String &content);

	String get_fs_doc_id() const;
protected:
	static void _bind_methods();
	void _notification(int p_what);

private:
	AutomergeFS *fs;
};
