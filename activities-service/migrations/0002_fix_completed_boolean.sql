ALTER TABLE activities
    ALTER COLUMN completed TYPE BOOLEAN USING (completed::boolean),
    ALTER COLUMN completed SET DEFAULT false;
