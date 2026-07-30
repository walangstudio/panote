-- Custom ordering, so a list can be arranged by hand rather than only sorted.
--
-- Ordering is per level: positions are only ever compared among siblings, so a
-- note in one folder and a note in another never contend. New rows default to 0
-- and fall back to the id tiebreak until the level is first arranged.
ALTER TABLE notes ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
ALTER TABLE folders ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;

-- Read together with folder_id / parent_id when listing a level.
CREATE INDEX IF NOT EXISTS idx_notes_folder_order ON notes(folder_id, sort_order);
CREATE INDEX IF NOT EXISTS idx_folders_parent_order ON folders(parent_id, sort_order);
