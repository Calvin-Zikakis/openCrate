-- What was agreed for year and label before 021 was agreed on the release's values. A track whose
-- own tag differs would otherwise look edited in openCrate and be offered for clearing in rekordbox;
-- with no agreed value, the first comparison takes rekordbox's side, as it does for a new track.
-- Its own step, because 021 had already run on some libraries when this was added.
DELETE FROM merge_base WHERE field IN ('year', 'label');
