-- The tag reader now keeps every artist, not just the first. Forget when each file's tags
-- were read, so the next scan reads them again and updates the tracks. Files are only read.
UPDATE file SET tags_mtime_ms = NULL;
