-- Users: the authenticated actors. Role is 'user' or 'admin' (admin ⊃ user).
CREATE TABLE users (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email         TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'admin')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Case-insensitive uniqueness on email so "A@x.com" and "a@x.com" can't both register.
CREATE UNIQUE INDEX idx_users_email_lower ON users (lower(email));
