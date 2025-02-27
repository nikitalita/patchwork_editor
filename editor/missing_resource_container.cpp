/**************************************************************************/
/*  missing_resource.cpp                                                  */
/**************************************************************************/
/*                         This file is part of:                          */
/*                             GODOT ENGINE                               */
/*                        https://godotengine.org                         */
/**************************************************************************/
/* Copyright (c) 2014-present Godot Engine contributors (see AUTHORS.md). */
/* Copyright (c) 2007-2014 Juan Linietsky, Ariel Manzur.                  */
/*                                                                        */
/* Permission is hereby granted, free of charge, to any person obtaining  */
/* a copy of this software and associated documentation files (the        */
/* "Software"), to deal in the Software without restriction, including    */
/* without limitation the rights to use, copy, modify, merge, publish,    */
/* distribute, sublicense, and/or sell copies of the Software, and to     */
/* permit persons to whom the Software is furnished to do so, subject to  */
/* the following conditions:                                              */
/*                                                                        */
/* The above copyright notice and this permission notice shall be         */
/* included in all copies or substantial portions of the Software.        */
/*                                                                        */
/* THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,        */
/* EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF     */
/* MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. */
/* IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY   */
/* CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,   */
/* TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE      */
/* SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.                 */
/**************************************************************************/

#include "missing_resource_container.h"

bool FakeInspectorResource::_set(const StringName &p_name, const Variant &p_value) {
	if (is_recording_properties()) {
		file_diff_map.insert(p_name, p_value);
		return true; //always valid to set (add)
	} else {
		if (!file_diff_map.has(p_name)) {
			return false;
		}

		file_diff_map[p_name] = p_value;
		return true;
	}
}

bool FakeInspectorResource::_get(const StringName &p_name, Variant &r_ret) const {
	if (!file_diff_map.has(p_name)) {
		return false;
	}
	r_ret = file_diff_map[p_name];
	return true;
}

void FakeInspectorResource::add_props_to_list(const Dictionary &props, List<PropertyInfo> *p_list) const {
	for (const auto &K : props.keys()) {
		p_list->push_back(PropertyInfo(props[K].get_type(), K));
	}
}

void FakeInspectorResource::_get_property_list(List<PropertyInfo> *p_list) const {
	for (const auto &E : file_diff_map) {
		// push back a divider
		Dictionary d = E.value;
		String type = d["type"];
		if (type == "deleted") {
			p_list->push_back(PropertyInfo(Variant::NIL, E.key + " (deleted)", PROPERTY_HINT_NONE, E.key + " (deleted)", PROPERTY_USAGE_CATEGORY));
			continue;
		}
		if (type == "added") {
			p_list->push_back(PropertyInfo(Variant::NIL, E.key + " (added)", PROPERTY_HINT_NONE, E.key + " (added)", PROPERTY_USAGE_CATEGORY));
			continue;
		}
		if (type == "type_changed") {
			p_list->push_back(PropertyInfo(Variant::NIL, E.key + " (type changed)", PROPERTY_HINT_NONE, E.key + " (type changed)", PROPERTY_USAGE_CATEGORY));
			p_list->push_back(PropertyInfo(Variant::STRING, "old_type", PROPERTY_HINT_NONE, "old_type", PROPERTY_USAGE_EDITOR));
			p_list->push_back(PropertyInfo(Variant::STRING, "new_type", PROPERTY_HINT_NONE, "new_type", PROPERTY_USAGE_EDITOR));
			continue;
		}
		p_list->push_back(PropertyInfo(Variant::NIL, E.key, PROPERTY_HINT_NONE, E.key, PROPERTY_USAGE_CATEGORY));
		if (type == "resource_changed") {
			add_props_to_list(d["props"], p_list);
			continue;
		}
		if (type == "scene_changed") {
			Array node_diffs = d["nodes"];
			for (int i = 0; i < node_diffs.size(); i++) {
				Dictionary node_diff = node_diffs[i];
				String node_path = node_diff["path"];
				String type = node_diff["type"];
				if (type == "node_deleted") {
					p_list->push_back(PropertyInfo(Variant::NIL, E.key + " (Deleted)", PROPERTY_HINT_NONE, E.key + " (Deleted)", PROPERTY_USAGE_SUBGROUP));
				} else if (type == "node_added") {
					p_list->push_back(PropertyInfo(Variant::NIL, E.key + " (Added)", PROPERTY_HINT_NONE, E.key + " (Added)", PROPERTY_USAGE_SUBGROUP));
				} else if (type == "node_changed") {
					p_list->push_back(PropertyInfo(Variant::NIL, node_path, PROPERTY_HINT_NONE, node_path, PROPERTY_USAGE_CATEGORY));
					Dictionary node_props = node_diff["props"];
					add_props_to_list(node_props, p_list);
				}
			}
		}
	}
}

void FakeInspectorResource::add_file_diff(const String &file, const Dictionary &props) {
	file_diff_map.insert(file, props);
}

void FakeInspectorResource::set_original_class(const String &p_class) {
	original_class = p_class;
}

String FakeInspectorResource::get_original_class() const {
	return original_class;
}

void FakeInspectorResource::set_recording_properties(bool p_enable) {
	recording_properties = p_enable;
}

bool FakeInspectorResource::is_recording_properties() const {
	return recording_properties;
}

void FakeInspectorResource::_bind_methods() {
	ClassDB::bind_method(D_METHOD("set_original_class", "name"), &FakeInspectorResource::set_original_class);
	ClassDB::bind_method(D_METHOD("get_original_class"), &FakeInspectorResource::get_original_class);

	ClassDB::bind_method(D_METHOD("set_recording_properties", "enable"), &FakeInspectorResource::set_recording_properties);
	ClassDB::bind_method(D_METHOD("is_recording_properties"), &FakeInspectorResource::is_recording_properties);

	// Expose, but not save.
	ADD_PROPERTY(PropertyInfo(Variant::STRING, "original_class", PROPERTY_HINT_NONE, "", PROPERTY_USAGE_NONE), "set_original_class", "get_original_class");
	ADD_PROPERTY(PropertyInfo(Variant::BOOL, "recording_properties", PROPERTY_HINT_NONE, "", PROPERTY_USAGE_NONE), "set_recording_properties", "is_recording_properties");
}

FakeInspectorResource::FakeInspectorResource() {
}
