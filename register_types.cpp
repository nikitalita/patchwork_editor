/*************************************************************************/
/*  register_types.cpp                                                   */
/*************************************************************************/

#include "register_types.h"
#include "editor/editor_node.h"
#include "editor/patchwork_editor.h"

void patchwork_editor_init_callback() {
	EditorNode *editor = EditorNode::get_singleton();
	editor->add_child(memnew(PatchworkEditor(editor)));
}

void initialize_patchwork_editor_module(ModuleInitializationLevel p_level) {
	// Engine::get_singleton()->add_singleton(Engine::Singleton("GDRESettings", GDRESettings::get_singleton()));

	EditorNode::add_init_callback(&patchwork_editor_init_callback);
}

void uninitialize_patchwork_editor_module(ModuleInitializationLevel p_level) {
}
