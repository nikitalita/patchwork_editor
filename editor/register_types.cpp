#include "register_types.h"
#include "godot_project.h"

void register_patchwork_editor_types() {
    ClassDB::register_class<GodotProjectWrapper>();
}

void unregister_patchwork_editor_types() {
}