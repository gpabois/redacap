CREATE TABLE permissions (
    id BYTEA PRIMARY KEY,
    subject_user_id BYTEA REFERENCES users(id) ON DELETE CASCADE,
    subject_group_id BYTEA REFERENCES groups(id) ON DELETE CASCADE,
    resource_type TEXT NOT NULL,
    resource_id BYTEA,
    resource_group_id BYTEA REFERENCES groups(id) ON DELETE CASCADE,
    action TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT permissions_subject_exclusive CHECK (
        (subject_user_id IS NOT NULL)::int + (subject_group_id IS NOT NULL)::int = 1
    ),
    CONSTRAINT permissions_resource_exclusive CHECK (
        resource_id IS NULL OR resource_group_id IS NULL
    )
);

CREATE INDEX permissions_subject_user_idx ON permissions (subject_user_id);
CREATE INDEX permissions_subject_group_idx ON permissions (subject_group_id);
