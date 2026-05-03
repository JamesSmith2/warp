CREATE TABLE workspace_groups (
  id INTEGER PRIMARY KEY NOT NULL,
  window_id INTEGER NOT NULL,
  group_index INTEGER NOT NULL CHECK (group_index >= 0),
  name TEXT NOT NULL,
  active_tab_index INTEGER NOT NULL CHECK (active_tab_index >= 0),
  FOREIGN KEY(window_id) REFERENCES windows(id)
);

ALTER TABLE windows ADD COLUMN active_workspace_group_index INTEGER;
ALTER TABLE tabs ADD COLUMN workspace_group_id INTEGER REFERENCES workspace_groups(id);
