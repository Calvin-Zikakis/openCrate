-- Which shape a stored waveform has, so a change of shape is noticed even at the same length.
-- 0 is the old one-colour peaks; those are made again in the current format on first use.
ALTER TABLE waveform ADD COLUMN format INTEGER NOT NULL DEFAULT 0;
