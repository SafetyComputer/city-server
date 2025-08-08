-- Your SQL goes here
CREATE TYPE Winner AS ENUM ('Blue', 'Green', 'Draw');

CREATE TABLE matches (
    id SERIAL NOT NULL PRIMARY KEY,
    player_blue INT NOT NULL,
    player_green INT NOT NULL,
    winner Winner NOT NULL,
    history VARCHAR(10000) NOT NULL,
    FOREIGN KEY (player_blue) REFERENCES users(id),
    FOREIGN KEY (player_green) REFERENCES users(id)
);