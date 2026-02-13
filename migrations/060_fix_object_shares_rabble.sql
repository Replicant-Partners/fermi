-- Allow 'rabble' as an object_type in object_shares (needed for private rabble invites)
ALTER TABLE object_shares DROP CONSTRAINT IF EXISTS object_shares_object_type_check;
ALTER TABLE object_shares ADD CONSTRAINT object_shares_object_type_check
  CHECK (object_type IN ('agent', 'capability', 'forecast', 'index', 'repo', 'file', 'rabble'));
