-- One logical database per workspace service, mirroring the CI list in
-- .github/workflows/rust.yml ("for db in ..." in the Clippy/Test and
-- Coverage jobs). A drift guard in accounts-service/tests/ asserts this
-- file and the workflow stay in agreement, so edit both together.
--
-- Runs once, on first boot of the docker-compose `db` service (files in
-- /docker-entrypoint-initdb.d execute only when the data directory is empty).
CREATE DATABASE accounts;
CREATE DATABASE contacts;
CREATE DATABASE activities;
CREATE DATABASE workflows;
CREATE DATABASE connections;
CREATE DATABASE opportunities;
CREATE DATABASE reports;
CREATE DATABASE documents;
CREATE DATABASE spend;
CREATE DATABASE projects;
CREATE DATABASE audit;
