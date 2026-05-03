ALTER TABLE tabs DROP COLUMN workspace_group_id;
ALTER TABLE windows DROP COLUMN active_workspace_group_index;

DROP TABLE workspace_groups;
