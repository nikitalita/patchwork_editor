#include "editor/patchwork_editor.h"
#include "modules/patchwork_editor/editor/patchwork_editor.h"

//	PatchworkEditor(EditorNode *p_editor);

PatchworkEditor::PatchworkEditor() {
}

PatchworkEditor::~PatchworkEditor() {
}

PatchworkEditor::PatchworkEditor(EditorNode *p_editor) {
	editor = p_editor;
}