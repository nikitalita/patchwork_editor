#include "patchwork_editor.h"
#include "automerge_fs.h"

#include <core/variant/variant.h>
#include <core/io/json.h>
#include <core/io/resource_loader.h>
#include <core/io/file_access.h>
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
			fs->save(scene->get_scene_file_path(), contents);
		}
		file->close();
	}
	DirAccess::remove_absolute(temp_file);
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

	fs = AutomergeFSWrapper::instance_and_create(PW_PROJECT_URL);
	this->add_child(fs);
	// EditorFileSystem::get_singleton()->connect("filesystem_changed", callable_mp(this, &PatchworkEditor::signal_callback));
	EditorUndoRedoManager::get_singleton()->connect(SNAME("history_changed"), callable_mp(this, &PatchworkEditor::_on_history_changed));
	fs->start();
}
