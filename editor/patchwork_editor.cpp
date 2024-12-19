#include "patchwork_editor.h"
extern "C" {
#include <automerge-c/automerge.h>
#include <automerge-c/utils/stack.h>
#include <automerge-c/utils/stack_callback_data.h>
}
PatchworkEditor::PatchworkEditor() {
}

PatchworkEditor::~PatchworkEditor() {
}

static bool abort_cb(AMstack **, void *) {
	return true;
}

PatchworkEditor::PatchworkEditor(EditorNode *p_editor) {
	editor = p_editor;
	// Just testing linkage to automerge-c
	AMstack *stack = NULL;
	AMdoc *doc1;
	AMitemToDoc(AMstackItem(&stack, AMcreate(NULL), abort_cb, AMexpect(AM_VAL_TYPE_DOC)), &doc1);
}