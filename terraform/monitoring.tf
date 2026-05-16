# v1.10 — FinOps Monitoring & Cost Dashboards
# Cloud Monitoring dashboards for per-service cost visibility and optimization recommendations

# Custom dashboard for microservices cost & performance metrics
resource "google_monitoring_dashboard" "finops_dashboard" {
  count = var.billing_account_id != "" ? 1 : 0
  dashboard_json = jsonencode({
    displayName = "Portfolio FinOps Dashboard"
    mosaicLayout = {
      columns = 12
      tiles = [
        # Row 1: Summary tiles
        {
          width  = 3
          height = 2
          widget = {
            title = "Monthly Budget"
            scorecard = {
              timeSeriesQuery = {
                timeSeriesFilter = {
                  filter = "metric.type=\"billing.googleapis.com/billing_account_charges\" resource.type=\"billing_account\""
                }
              }
              sparkChartView = {
                sparkChartType = "SPARK_LINE"
              }
            }
          }
        },
        {
          xPos   = 3
          width  = 3
          height = 2
          widget = {
            title = "Estimated Cost Trend"
            xyChart = {
              dataSets = [
                {
                  timeSeriesQuery = {
                    timeSeriesFilter = {
                      filter = "metric.type=\"billing.googleapis.com/billing_account_charges\""
                    }
                  }
                  plotType = "LINE"
                }
              ]
            }
          }
        },
        {
          xPos   = 6
          width  = 3
          height = 2
          widget = {
            title = "Cloud Run Instances Running"
            xyChart = {
              dataSets = [
                {
                  timeSeriesQuery = {
                    timeSeriesFilter = {
                      filter = "metric.type=\"run.googleapis.com/instance_count\" resource.type=\"cloud_run_revision\""
                    }
                  }
                  plotType = "LINE"
                }
              ]
            }
          }
        },
        {
          xPos   = 9
          width  = 3
          height = 2
          widget = {
            title = "Avg Response Time"
            xyChart = {
              dataSets = [
                {
                  timeSeriesQuery = {
                    timeSeriesFilter = {
                      filter = "metric.type=\"run.googleapis.com/request_latencies\" resource.type=\"cloud_run_revision\""
                    }
                  }
                  plotType = "LINE"
                }
              ]
            }
          }
        },

        # Row 2: Per-service cost breakdown
        {
          yPos   = 2
          width  = 6
          height = 3
          widget = {
            title = "Cloud Run Billed Time (by service)"
            xyChart = {
              dataSets = [
                {
                  timeSeriesQuery = {
                    timeSeriesFilter = {
                      filter = "metric.type=\"run.googleapis.com/execution_times\" resource.type=\"cloud_run_revision\""
                      aggregation = {
                        alignmentPeriod  = "60s"
                        perSeriesAligner = "ALIGN_SUM"
                        groupByFields = [
                          "resource.service_name"
                        ]
                      }
                    }
                  }
                  plotType = "STACKED_AREA"
                }
              ]
            }
          }
        },
        {
          xPos   = 6
          yPos   = 2
          width  = 6
          height = 3
          widget = {
            title = "Cold Start Count (by service)"
            xyChart = {
              dataSets = [
                {
                  timeSeriesQuery = {
                    timeSeriesFilter = {
                      filter = "resource.type=\"cloud_run_revision\" metric.type=\"run.googleapis.com/request_count\""
                      aggregation = {
                        alignmentPeriod  = "300s"
                        perSeriesAligner = "ALIGN_RATE"
                        groupByFields = [
                          "resource.service_name"
                        ]
                      }
                    }
                  }
                  plotType = "LINE"
                }
              ]
            }
          }
        },

        # Row 3: Database and resource utilization
        {
          yPos   = 5
          width  = 4
          height = 3
          widget = {
            title = "Cloud SQL CPU %"
            xyChart = {
              dataSets = [
                {
                  timeSeriesQuery = {
                    timeSeriesFilter = {
                      filter = "metric.type=\"cloudsql.googleapis.com/database/cpu/utilization\" resource.type=\"cloudsql_database\""
                    }
                  }
                  plotType = "LINE"
                }
              ]
            }
          }
        },
        {
          xPos   = 4
          yPos   = 5
          width  = 4
          height = 3
          widget = {
            title = "Cloud SQL Memory %"
            xyChart = {
              dataSets = [
                {
                  timeSeriesQuery = {
                    timeSeriesFilter = {
                      filter = "metric.type=\"cloudsql.googleapis.com/database/memory/utilization\" resource.type=\"cloudsql_database\""
                    }
                  }
                  plotType = "LINE"
                }
              ]
            }
          }
        },
        {
          xPos   = 8
          yPos   = 5
          width  = 4
          height = 3
          widget = {
            title = "Artifact Registry Storage (GB)"
            xyChart = {
              dataSets = [
                {
                  timeSeriesQuery = {
                    timeSeriesFilter = {
                      filter = "metric.type=\"artifactregistry.googleapis.com/repository/storage\" resource.type=\"artifactregistry.googleapis.com/Repository\""
                    }
                  }
                  plotType = "LINE"
                }
              ]
            }
          }
        },

        # Row 4: Error and performance metrics
        {
          yPos   = 8
          width  = 4
          height = 2
          widget = {
            title = "Error Rate (5xx) %"
            xyChart = {
              dataSets = [
                {
                  timeSeriesQuery = {
                    timeSeriesFilter = {
                      filter = "metric.type=\"run.googleapis.com/request_count\" resource.type=\"cloud_run_revision\" metric.response_code_class=\"5xx\""
                    }
                  }
                  plotType = "LINE"
                }
              ]
            }
          }
        },
        {
          xPos   = 4
          yPos   = 8
          width  = 4
          height = 2
          widget = {
            title = "P99 Latency (ms)"
            xyChart = {
              dataSets = [
                {
                  timeSeriesQuery = {
                    timeSeriesFilter = {
                      filter = "metric.type=\"run.googleapis.com/request_latencies\" resource.type=\"cloud_run_revision\""
                      aggregation = {
                        alignmentPeriod      = "60s"
                        perSeriesAligner     = "ALIGN_PERCENTILE_99"
                        groupByFields = [
                          "resource.service_name"
                        ]
                      }
                    }
                  }
                  plotType = "LINE"
                }
              ]
            }
          }
        },
        {
          xPos   = 8
          yPos   = 8
          width  = 4
          height = 2
          widget = {
            title = "DB Connections Active"
            xyChart = {
              dataSets = [
                {
                  timeSeriesQuery = {
                    timeSeriesFilter = {
                      filter = "metric.type=\"cloudsql.googleapis.com/database/mysql/connections\" resource.type=\"cloudsql_database\""
                    }
                  }
                  plotType = "LINE"
                }
              ]
            }
          }
        }
      ]
    }
  })
}

# Prometheus-compatible metrics export for Grafana scraping
# (Can be consumed by Grafana via Google Cloud Monitoring plugin)
output "monitoring_dashboard_link" {
  description = "URL to Cloud Monitoring dashboard for FinOps metrics"
  value = var.billing_account_id != "" ? "https://console.cloud.google.com/monitoring/dashboards/custom/${google_monitoring_dashboard.finops_dashboard[0].id}?project=${data.google_project.current.project_id}" : "No billing account configured"
}
