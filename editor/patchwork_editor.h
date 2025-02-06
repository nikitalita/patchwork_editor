#pragma once
#include "core/object/class_db.h"
#include "editor/editor_node.h"
#include "scene/main/node.h"
class GodotProject;

class PatchworkEditor : public Node {
	GDCLASS(PatchworkEditor, Node);

private:
	EditorNode *editor = nullptr;
	GodotProject *fs = nullptr;
	void _on_filesystem_changed();
	void _on_resources_reloaded();
	void _on_history_changed();
	void handle_change(const String &resource_path, const NodePath &node_path, HashMap<String, Variant> properties);
	void _on_file_changed(Dictionary dict);
	static bool unsaved_files_open();
	static bool detect_utf8(const PackedByteArray &p_utf8_buf);
	static Vector<String> get_recursive_dir_list(const String &p_dir, const Vector<String> &wildcards = {}, bool absolute = true, const String &rel = "");

protected:
	void _notification(int p_what);
	static void _bind_methods();

public:
	PatchworkEditor(EditorNode *p_editor);


public:
	PatchworkEditor();
	~PatchworkEditor();
};