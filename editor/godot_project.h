#pragma once
#include "core/string/print_string.h"
#include "core/variant/variant.h"
#include "patchwork_rust/patchwork_rust.h"
#include "scene/main/node.h"

class GodotProject : public Node {
	GDCLASS(GodotProject, Node);

	static void _signal_callback(void *signal_user_data, const char *signal, const char *const *args, size_t p_args_len);
	void signal_callback(const String &signal, const Vector<String> &args);

public:
	static GodotProject *create(const String &maybe_fs_doc_id);
	// Don't use this, uses instance_and_create instead
	GodotProject();
	~GodotProject();

	void init(const String &maybe_fs_doc_id);

	void process();

	void stop();

	Error save_file(const String &path, const Variant &content);

	Variant get_file(const String &path);

	String get_fs_doc_id() const;

	TypedArray<Dictionary> get_branches();

	void checkout_branch(const String &branch_id);

	String create_branch(const String &name);

	String get_checked_out_branch_id() const;
	Vector<String> list_all_files();
	Vector<String> get_heads();

protected:
	static void _bind_methods();
	void _notification(int p_what);

private:
	GodotProject_rs *fs;
};
