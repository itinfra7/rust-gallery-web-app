CREATE TABLE IF NOT EXISTS images (
    id UUID PRIMARY KEY,
    filename TEXT NOT NULL UNIQUE,
    upload_date TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    title TEXT,
    tags TEXT[] DEFAULT '{}'
);

ALTER TABLE images ADD COLUMN IF NOT EXISTS like_count INT NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS likes (
    image_id UUID NOT NULL REFERENCES images(id) ON DELETE CASCADE,
    ip_address TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (image_id, ip_address)
);

DROP TRIGGER IF EXISTS tsvectorupdate ON images;
ALTER TABLE images DROP COLUMN IF EXISTS search_vector;

CREATE INDEX IF NOT EXISTS idx_images_title ON images(title);
CREATE INDEX IF NOT EXISTS idx_images_upload_date ON images(upload_date DESC);
CREATE INDEX IF NOT EXISTS idx_images_tags ON images USING GIN(tags);
CREATE INDEX IF NOT EXISTS idx_images_like_count ON images(like_count DESC);
