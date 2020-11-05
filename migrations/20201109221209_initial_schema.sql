CREATE TABLE IF NOT EXISTS photos (
	id SERIAL PRIMARY KEY,

	-- File name without file extension, for deduplication purposes.
	file_stem VARCHAR NOT NULL,

	title VARCHAR,
	taken_timestamp VARCHAR,

	-- Percentage offset inte the photo to use when photo is being cropped due to size constraints.
	height_offset INTEGER NOT NULL DEFAULT 50,
	CHECK (height_offset >= 0),
	CHECK (height_offset <= 100),

	tags VARCHAR[] NOT NULL DEFAULT '{}',
	published BOOLEAN DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_photos_tags ON photos USING GIN(tags);


CREATE TABLE IF NOT EXISTS sources (
	photo_id INTEGER REFERENCES photos (id) ON DELETE CASCADE ON UPDATE CASCADE,

	width INTEGER NOT NULL,
	CHECK (width < 10000),

	height INTEGER NOT NULL,
	CHECK (height < 10000),

	url VARCHAR NOT NULL,

	UNIQUE (photo_id, width, height),
	UNIQUE (url)
);


CREATE TABLE IF NOT EXISTS secret_keys (
	secret_key VARCHAR PRIMARY KEY
)
