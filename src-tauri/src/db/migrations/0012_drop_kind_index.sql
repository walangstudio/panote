-- Nothing has ever queried notes by kind; the index only cost write time.
DROP INDEX IF EXISTS idx_notes_kind;
