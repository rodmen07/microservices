ALTER TABLE activities
    ALTER COLUMN completed TYPE BOOLEAN USING (completed <> 0),
    ALTER COLUMN completed SET DEFAULT false;
