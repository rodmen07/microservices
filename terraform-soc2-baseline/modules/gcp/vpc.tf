# ---------------------------------------------------------------------------
# CC6.1 / A1.2 — VPC Segmentation (GCP)
#
# Private VPC with Cloud SQL on private IP (no public exposure).
# Cloud Run services connect via Service Connect or Cloud SQL Auth Proxy.
# Firewall rules follow deny-by-default with explicit allow rules.
# ---------------------------------------------------------------------------

resource "google_compute_network" "vpc" {
  project                 = var.project_id
  name                    = var.network_name
  auto_create_subnetworks = false
  description             = "SOC 2 baseline VPC. Managed by terraform-soc2-baseline."
}

resource "google_compute_subnetwork" "private" {
  project       = var.project_id
  name          = "${var.network_name}-private"
  region        = var.region
  network       = google_compute_network.vpc.id
  ip_cidr_range = var.subnet_cidr

  # Enable private Google access so VMs can reach Google APIs without public IPs
  private_ip_google_access = true

  log_config {
    aggregation_interval = "INTERVAL_5_SEC"
    flow_sampling        = 0.5
    metadata             = "INCLUDE_ALL_METADATA"
  }
}

# Private services access for Cloud SQL
resource "google_compute_global_address" "private_ip_range" {
  project       = var.project_id
  name          = "${var.network_name}-private-ip-range"
  purpose       = "VPC_PEERING"
  address_type  = "INTERNAL"
  prefix_length = 16
  network       = google_compute_network.vpc.id
}

resource "google_service_networking_connection" "private_sql" {
  network                 = google_compute_network.vpc.id
  service                 = "servicenetworking.googleapis.com"
  reserved_peering_ranges = [google_compute_global_address.private_ip_range.name]
}

# ---------------------------------------------------------------------------
# Firewall rules — deny all ingress by default, allow only what's needed
# ---------------------------------------------------------------------------

# Block all ingress from the internet to internal resources
resource "google_compute_firewall" "deny_all_ingress" {
  project  = var.project_id
  name     = "${var.network_name}-deny-all-ingress"
  network  = google_compute_network.vpc.id
  priority = 65534
  direction = "INGRESS"

  deny {
    protocol = "all"
  }

  source_ranges = ["0.0.0.0/0"]
  log_config {
    metadata = "INCLUDE_ALL_METADATA"
  }
}

# Allow internal traffic within the VPC subnet
resource "google_compute_firewall" "allow_internal" {
  project   = var.project_id
  name      = "${var.network_name}-allow-internal"
  network   = google_compute_network.vpc.id
  priority  = 1000
  direction = "INGRESS"

  allow {
    protocol = "tcp"
    ports    = ["0-65535"]
  }
  allow {
    protocol = "udp"
    ports    = ["0-65535"]
  }
  allow {
    protocol = "icmp"
  }

  source_ranges = [var.subnet_cidr]
}

# Allow health checks from Google load balancer IP ranges
resource "google_compute_firewall" "allow_health_checks" {
  project   = var.project_id
  name      = "${var.network_name}-allow-health-checks"
  network   = google_compute_network.vpc.id
  priority  = 1000
  direction = "INGRESS"

  allow {
    protocol = "tcp"
    ports    = ["8080", "8090", "443"]
  }

  source_ranges = ["35.191.0.0/16", "130.211.0.0/22"]
}
