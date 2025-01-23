#pragma once
#include "core/object/class_db.h"
#include "editor/editor_node.h"
#include "scene/main/node.h"
class GodotProjectWrapper;

class PatchworkEditor : public Node {
	GDCLASS(PatchworkEditor, Node);

private:
	EditorNode *editor = nullptr;
	GodotProjectWrapper *fs = nullptr;
	void _on_filesystem_changed();
	void _on_resources_reloaded();
	void _on_history_changed();
	void handle_change(const String &resource_path, const NodePath &node_path, HashMap<String, Variant> properties);
	void _on_file_changed(Dictionary dict);

protected:
	void _notification(int p_what);
public:
	PatchworkEditor(EditorNode *p_editor);


public:
	PatchworkEditor();
	~PatchworkEditor();
};