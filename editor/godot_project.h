#pragma once
#include "core/string/print_string.h"
#include "core/variant/variant.h"
#include "patchwork_rust/patchwork_rust.h"
#include "scene/main/node.h"

class GodotProjectWrapper : public Node {
	GDCLASS(GodotProjectWrapper, Node);

	static void _signal_callback(void *signal_user_data, const char *signal, const char* const * args, size_t args_len);
	void signal_callback(const String &signal, Dictionary args);

public:
	static GodotProjectWrapper *instance_and_create(const String &maybe_fs_doc_id);
	GodotProjectWrapper();
	~GodotProjectWrapper();

	void create(const String &maybe_fs_doc_id);

	void process();

	void stop();

	void save_file(const String &path, const String &content);

	String get_fs_doc_id() const;

	TypedArray<Dictionary> get_branches();

	void checkout_branch(const String &branch_id);

	String create_branch(const String &name);

	String get_checked_out_branch_id() const;

protected:
	static void _bind_methods();
	void _notification(int p_what);

private:
	GodotProject *fs;
};
