#include "patchwork_editor.h"
#include "godot_project.h"

#include <core/io/file_access.h>
#include <core/io/json.h>
#include <core/io/resource_loader.h>
#include <core/variant/variant.h>
#include <editor/editor_file_system.h>
#include <editor/editor_undo_redo_manager.h>
#include <scene/resources/packed_scene.h>

PatchworkEditor::PatchworkEditor() {
}

PatchworkEditor::~PatchworkEditor() {
}

void PatchworkEditor::_on_filesystem_changed() {
}

void PatchworkEditor::_on_resources_reloaded() {
}

void PatchworkEditor::_on_history_changed() {
	// get the current edited scene
	auto scene = EditorNode::get_singleton()->get_edited_scene();
	if (scene == nullptr) {
		return;
	}
	// pack the scene into a packed scene
	auto packed_scene = memnew(PackedScene);
	packed_scene->pack(scene);
	// temp file name with random name
	auto temp_file = "user://temp_" + itos(OS::get_singleton()->get_unix_time()) + ".tscn";
	Error err = ResourceSaver::save(packed_scene, temp_file);
	if (err != OK) {
		print_line("Error saving scene");
		return;
	}
	// open the file
	auto file = FileAccess::open(temp_file, FileAccess::READ);
	if (file.is_valid()) {
		auto contents = file->get_as_text();
		auto scene_path = scene->get_scene_file_path();
		if (scene_path == "res://main.tscn") {
			fs->save_file(scene->get_scene_file_path(), contents);
			// test getting the file
			// auto file_contents = fs->get_file(scene->get_scene_file_path());
			// if (file_contents != contents) {
			// 	print_line("File contents do not match");
			// } else {
			// 	print_line("Yay");
			// }
		}
		file->close();
	}
	DirAccess::remove_absolute(temp_file);
}

void PatchworkEditor::handle_change(const String &resource_path, const NodePath &node_path, HashMap<String, Variant> properties) {
	auto res = ResourceLoader::load(resource_path);
	if (!node_path.is_empty()) {
		Ref<PackedScene> scene = res;
		auto node_idx = scene->get_state()->find_node_by_path(node_path);
	}
}

void PatchworkEditor::_on_file_changed(Dictionary dict) {
	// let args = ["file_path", "res://main.tscn",
	// "node_path", node_path.as_str(),
	// "type", "node_deleted",
	// ];
	auto file_path = dict["file_path"];
	auto node_path = dict["node_path"];
}

void PatchworkEditor::_notification(int p_what) {
	switch (p_what) {
		case NOTIFICATION_READY: {
			print_line("Entered tree");
			break;
		}
	}
}

#define PW_PROJECT_URL "3M3FmnUWqNuQEath9n6DXEYdNaTm"

PatchworkEditor::PatchworkEditor(EditorNode *p_editor) {
	editor = p_editor;
	EditorUndoRedoManager::get_singleton()->connect(SNAME("history_changed"), callable_mp(this, &PatchworkEditor::_on_history_changed));

	fs = GodotProjectWrapper::instance_and_create("");
	this->add_child(fs);
	// EditorFileSystem::get_singleton()->connect("filesystem_changed", callable_mp(this, &PatchworkEditor::signal_callback));
}
