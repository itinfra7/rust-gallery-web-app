CREATE TABLE IF NOT EXISTS comment_users (
    id UUID PRIMARY KEY,
    ip_address TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(ip_address, fingerprint)
);

CREATE TABLE IF NOT EXISTS comments (
    id UUID PRIMARY KEY,
    image_id UUID NOT NULL REFERENCES images(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES comment_users(id) ON DELETE CASCADE,
    content VARCHAR(100) NOT NULL,
    user_agent TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_comments_image_id ON comments(image_id);
CREATE INDEX IF NOT EXISTS idx_comments_created_at ON comments(created_at);
