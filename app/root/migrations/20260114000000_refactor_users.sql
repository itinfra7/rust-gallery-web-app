DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_tables WHERE schemaname = 'public' AND tablename = 'users') THEN
        IF EXISTS (SELECT FROM pg_tables WHERE schemaname = 'public' AND tablename = 'comment_users') THEN
            ALTER TABLE comment_users RENAME TO users;
        ELSE
            CREATE TABLE users (
                id UUID PRIMARY KEY,
                ip_address TEXT NOT NULL,
                fingerprint TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE(ip_address, fingerprint)
            );
        END IF;
    END IF;

    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name='users' AND column_name='last_activity_at') THEN
        ALTER TABLE users ADD COLUMN last_activity_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
    END IF;

    IF EXISTS (SELECT FROM information_schema.columns WHERE table_name='likes' AND column_name='ip_address') THEN
        INSERT INTO users (id, ip_address, fingerprint)
        SELECT gen_random_uuid(), ip_address, 'legacy_migration'
        FROM likes
        WHERE ip_address NOT IN (SELECT ip_address FROM users)
        GROUP BY ip_address;

        CREATE TABLE IF NOT EXISTS likes_new (
            image_id UUID NOT NULL REFERENCES images(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (image_id, user_id)
        );

        INSERT INTO likes_new (image_id, user_id)
        SELECT l.image_id, u.id
        FROM likes l
        JOIN users u ON l.ip_address = u.ip_address
        ON CONFLICT DO NOTHING;

        DROP TABLE likes;
        ALTER TABLE likes_new RENAME TO likes;

        UPDATE images
        SET like_count = (
            SELECT COUNT(*)
            FROM likes
            WHERE likes.image_id = images.id
        );
    END IF;
END $$;
