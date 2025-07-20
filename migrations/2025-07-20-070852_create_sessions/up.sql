-- Your SQL goes here
CREATE TABLE sessions (
    id SERIAL NOT NULL PRIMARY KEY,
    user_id INT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id),
    auth_id CHAR(32) NOT NULL,
    start_time TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    active BOOLEAN NOT NULL
);