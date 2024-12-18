#pragma once
#include "core/object/class_db.h"
#ifdef TOOLS_ENABLED
#include "editor/editor_node.h"
#endif
#include "scene/main/node.h"
class PatchworkEditor : public Node {
	GDCLASS(PatchworkEditor, Node);
#ifdef TOOLS_ENABLED
private:
	EditorNode *editor = nullptr;

public:
	PatchworkEditor(EditorNode *p_editor);
#endif

public:
	PatchworkEditor();
	~PatchworkEditor();
};