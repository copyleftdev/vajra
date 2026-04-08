//! Golden test corpus generator for Vajra.
//!
//! Generates realistic synthetic data across every domain and edge case
//! Vajra handles, then runs Vajra analysis against all generated files
//! and captures the output as golden files for regression testing.
//!
//! Usage: `cargo run -p vajra-cli --bin generate-corpus`

use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use vajra_anomaly::AnomalyAnalyzer;
use vajra_core::formats::parse_file_auto;
use vajra_drift::full_drift;
use vajra_essence::{EngineerProfile, EssenceBuilder};
use vajra_fingerprint::FingerprintAnalyzer;
use vajra_stats::StatsAnalyzer;
use vajra_types::traits::{Analyzer, ConcernProfile, OutputFormat};
use vajra_types::Document;

// ---------------------------------------------------------------------------
// Deterministic pseudo-random number generator (xorshift64)
// ---------------------------------------------------------------------------

struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }

    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next_u64() % (hi - lo)
    }

    fn range_f64(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next_f64() * (hi - lo)
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        let idx = self.next_u64() as usize % items.len();
        &items[idx]
    }

    /// Generate a value following approximate Benford's Law distribution.
    fn benford_amount(&mut self) -> f64 {
        // Use inverse CDF of log-uniform distribution
        let u = self.next_f64();
        let val = 10.0_f64.powf(u * 4.0); // range 1..10000
        (val * 100.0).round() / 100.0
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let corpus_dir = PathBuf::from("corpus");

    eprintln!("=== Vajra Golden Test Corpus Generator ===");
    eprintln!("Output directory: {}", corpus_dir.display());

    // Phase 1: Generate corpus files
    eprintln!("\n--- Phase 1: Generating corpus files ---");
    generate_all(&corpus_dir);

    // Phase 2: Capture golden output
    eprintln!("\n--- Phase 2: Capturing golden output ---");
    capture_all_golden(&corpus_dir);

    eprintln!("\n=== Done ===");
}

// ---------------------------------------------------------------------------
// Phase 1: Generate all corpus files
// ---------------------------------------------------------------------------

fn generate_all(root: &Path) {
    generate_medical(root);
    generate_api(root);
    generate_config(root);
    generate_telemetry(root);
    generate_financial(root);
    generate_edge_cases(root);
    generate_documents(root);
}

fn ensure_dir(path: &Path) {
    fs::create_dir_all(path).unwrap_or_else(|e| {
        eprintln!("  failed to create {}: {e}", path.display());
    });
}

fn write_json(path: &Path, value: &Value) {
    let content = serde_json::to_string_pretty(value).unwrap_or_else(|e| {
        eprintln!("  failed to serialize {}: {e}", path.display());
        String::new()
    });
    fs::write(path, content).unwrap_or_else(|e| {
        eprintln!("  failed to write {}: {e}", path.display());
    });
    eprintln!("  generated: {}", path.display());
}

fn write_text(path: &Path, content: &str) {
    fs::write(path, content).unwrap_or_else(|e| {
        eprintln!("  failed to write {}: {e}", path.display());
    });
    eprintln!("  generated: {}", path.display());
}

// ---------------------------------------------------------------------------
// Category 1: Medical Claims
// ---------------------------------------------------------------------------

fn generate_medical(root: &Path) {
    let dir = root.join("medical");
    ensure_dir(&dir);

    // single_claim.json
    write_json(&dir.join("single_claim.json"), &make_single_claim());

    // claim_batch.ndjson
    write_text(&dir.join("claim_batch.ndjson"), &make_claim_batch());

    // eligibility_response.json
    write_json(
        &dir.join("eligibility_response.json"),
        &make_eligibility_response(),
    );

    // edi_835_translated.json
    write_json(
        &dir.join("edi_835_translated.json"),
        &make_edi_835_translated(),
    );
}

fn make_single_claim() -> Value {
    json!({
        "claim_id": "CLM-2025-000142",
        "claim_status": "paid",
        "filing_date": "2025-03-15",
        "patient": {
            "name": "Jane A. Doe",
            "date_of_birth": "1985-06-12",
            "subscriber_id": "SUB-9871234",
            "gender": "F",
            "address": {
                "street": "1234 Maple Drive",
                "city": "Springfield",
                "state": "IL",
                "zip": "62704"
            }
        },
        "provider": {
            "npi": "1234567890",
            "name": "Dr. Robert Smith",
            "taxonomy_code": "207Q00000X",
            "address": {
                "street": "500 Medical Center Blvd",
                "city": "Springfield",
                "state": "IL",
                "zip": "62701"
            }
        },
        "diagnoses": [
            {
                "code": "E11.9",
                "description": "Type 2 diabetes mellitus without complications",
                "type": "principal"
            },
            {
                "code": "I10",
                "description": "Essential (primary) hypertension",
                "type": "secondary"
            }
        ],
        "service_lines": [
            {
                "line_number": 1,
                "procedure_code": "99213",
                "service_date": "2025-03-10",
                "charge_amount": 150.00,
                "allowed_amount": 125.00,
                "paid_amount": 100.00,
                "adjustment_amount": 25.00,
                "status": "paid",
                "adjustment_reason": "CO-45"
            },
            {
                "line_number": 2,
                "procedure_code": "99214",
                "service_date": "2025-03-10",
                "charge_amount": 225.00,
                "allowed_amount": 190.00,
                "paid_amount": 152.00,
                "adjustment_amount": 38.00,
                "status": "paid",
                "adjustment_reason": "PR-1"
            },
            {
                "line_number": 3,
                "procedure_code": "99215",
                "service_date": "2025-03-11",
                "charge_amount": 300.00,
                "allowed_amount": 250.00,
                "paid_amount": 200.00,
                "adjustment_amount": 50.00,
                "status": "paid",
                "adjustment_reason": "CO-45"
            },
            {
                "line_number": 4,
                "procedure_code": "99213",
                "service_date": "2025-03-12",
                "charge_amount": 150.00,
                "allowed_amount": 125.00,
                "paid_amount": 100.00,
                "adjustment_amount": 25.00,
                "status": "denied",
                "adjustment_reason": "CO-45"
            },
            {
                "line_number": 5,
                "procedure_code": "99214",
                "service_date": "2025-03-13",
                "charge_amount": 225.00,
                "allowed_amount": 190.00,
                "paid_amount": 152.00,
                "adjustment_amount": 38.00,
                "status": "pending",
                "adjustment_reason": "PR-1"
            }
        ],
        "payer": {
            "name": "Blue Cross Blue Shield of Illinois",
            "payer_id": "BCBS-IL-001",
            "plan_type": "PPO"
        }
    })
}

fn make_claim_batch() -> String {
    let mut rng = Rng::new(42);
    let mut lines = Vec::with_capacity(50);

    let statuses = [
        "paid", "paid", "paid", "paid", "paid", "paid", "paid", "denied", "denied", "denied",
        "pending", "pending", "partial",
    ];
    let cpt_codes = ["99213", "99214", "99215", "99211", "99212"];
    let hcpcs_codes = ["J0120", "G0101", "A0021", "J3490"];
    let icd10_codes = [
        "E11.9", "I10", "J18.9", "M54.5", "Z00.00", "K21.0", "F32.9", "N18.3",
    ];
    let adj_reasons = ["CO-45", "PR-1", "OA-23", "PI-97"];
    let ndc_codes = ["12345-6789-01", "54321-1234-02", "99999-0001-01"];
    let first_names = [
        "John", "Jane", "Michael", "Sarah", "Robert", "Emily", "David", "Maria", "James", "Lisa",
    ];
    let last_names = [
        "Smith", "Johnson", "Williams", "Brown", "Jones", "Davis", "Miller", "Wilson", "Moore",
        "Taylor",
    ];

    for i in 0..50 {
        let num_lines = rng.range(1, 21) as usize;
        let status = *rng.pick(&statuses);
        let num_diag = rng.range(1, 5) as usize;

        let mut service_lines = Vec::new();
        for ln in 0..num_lines {
            let is_drug = rng.next_f64() < 0.15;
            let procedure_code: Value = if is_drug {
                json!(*rng.pick(&hcpcs_codes))
            } else {
                json!(*rng.pick(&cpt_codes))
            };

            // Planted anomaly: claims 10 and 11 have 10x charges
            let charge_multiplier = if i == 10 || i == 11 { 10.0 } else { 1.0 };
            let charge = (rng.range_f64(75.0, 400.0) * charge_multiplier * 100.0).round() / 100.0;

            // Type instability: some claims have null allowed_amount
            let allowed: Value = if rng.next_f64() < 0.1 {
                Value::Null
            } else {
                json!((charge * rng.range_f64(0.6, 0.95) * 100.0).round() / 100.0)
            };

            let paid = if let Some(a) = allowed.as_f64() {
                (a * rng.range_f64(0.7, 1.0) * 100.0).round() / 100.0
            } else {
                0.0
            };

            let mut line = json!({
                "line_number": ln + 1,
                "procedure_code": procedure_code,
                "service_date": format!("2025-{:02}-{:02}", rng.range(1, 13), rng.range(1, 29)),
                "charge_amount": charge,
                "allowed_amount": allowed,
                "paid_amount": paid,
                "adjustment_amount": (charge - paid * 100.0).abs().round() / 100.0,
                "status": status,
                "adjustment_reason": *rng.pick(&adj_reasons)
            });

            // Add NDC for drug-related lines
            if is_drug {
                line.as_object_mut()
                    .unwrap_or_else(|| unreachable!())
                    .insert("ndc_code".to_string(), json!(*rng.pick(&ndc_codes)));
            }

            service_lines.push(line);
        }

        let mut diagnoses = Vec::new();
        for d in 0..num_diag {
            // Planted anomaly: claim 25 has an unusual diagnosis code
            let code = if i == 25 && d == 0 {
                "Z99.89"
            } else {
                rng.pick(&icd10_codes)
            };
            diagnoses.push(json!({
                "code": code,
                "description": format!("Diagnosis for {}", code),
                "type": if d == 0 { "principal" } else { "secondary" }
            }));
        }

        let first = *rng.pick(&first_names);
        let last = *rng.pick(&last_names);
        let npi_suffix = rng.range(1000000, 9999999);

        // Inconsistent date formats on some claims
        let filing_date: Value = if i % 7 == 0 {
            json!(format!("{:02}/15/2025", rng.range(1, 13)))
        } else {
            json!(format!("2025-{:02}-15", rng.range(1, 13)))
        };

        let claim = json!({
            "claim_id": format!("CLM-2025-{:06}", i + 1),
            "claim_status": status,
            "filing_date": filing_date,
            "patient": {
                "name": format!("{} {}", first, last),
                "date_of_birth": format!("19{:02}-{:02}-{:02}", rng.range(50, 99), rng.range(1, 13), rng.range(1, 29)),
                "subscriber_id": format!("SUB-{:07}", rng.range(1000000, 9999999)),
                "gender": if rng.next_f64() < 0.5 { "F" } else { "M" }
            },
            "provider": {
                "npi": format!("1{:09}", npi_suffix),
                "name": format!("Dr. {} {}", first, last),
                "taxonomy_code": "207Q00000X"
            },
            "diagnoses": diagnoses,
            "service_lines": service_lines,
            "payer": {
                "name": "Blue Cross Blue Shield",
                "payer_id": format!("BCBS-{:03}", rng.range(1, 50))
            }
        });

        lines.push(serde_json::to_string(&claim).unwrap_or_default());
    }

    lines.join("\n")
}

fn make_eligibility_response() -> Value {
    json!({
        "transaction_id": "ELIG-2025-001",
        "status": "active",
        "subscriber": {
            "name": "John Q. Public",
            "member_id": "MBR-5551234",
            "date_of_birth": "1978-04-22",
            "gender": "M",
            "relationship": "self"
        },
        "dependents": [
            {
                "name": "Mary J. Public",
                "date_of_birth": "1980-09-15",
                "gender": "F",
                "relationship": "spouse"
            },
            {
                "name": "Tommy R. Public",
                "date_of_birth": "2010-01-30",
                "gender": "M",
                "relationship": "child"
            }
        ],
        "plan": {
            "plan_name": "Preferred PPO Gold",
            "plan_id": "PLAN-PPO-GOLD-2025",
            "group_number": "GRP-887766",
            "effective_date": "2025-01-01",
            "termination_date": null
        },
        "benefits": {
            "medical": {
                "in_network": {
                    "deductible": {"individual": 1500.00, "family": 3000.00, "remaining": 750.00},
                    "out_of_pocket_max": {"individual": 6000.00, "family": 12000.00},
                    "copay": {"primary_care": 25.00, "specialist": 50.00, "urgent_care": 75.00, "emergency": 250.00},
                    "coinsurance": 0.20
                },
                "out_of_network": {
                    "deductible": {"individual": 3000.00, "family": 6000.00, "remaining": 3000.00},
                    "out_of_pocket_max": {"individual": 12000.00, "family": 24000.00},
                    "copay": null,
                    "coinsurance": 0.40
                }
            },
            "dental": {
                "in_network": {
                    "deductible": {"individual": 50.00},
                    "annual_maximum": 2000.00,
                    "preventive_coverage": 1.0,
                    "basic_coverage": 0.80,
                    "major_coverage": 0.50
                },
                "out_of_network": {
                    "deductible": {"individual": 100.00},
                    "annual_maximum": 1500.00,
                    "preventive_coverage": 0.80,
                    "basic_coverage": 0.60,
                    "major_coverage": 0.40
                }
            },
            "vision": {
                "in_network": {
                    "exam_copay": 10.00,
                    "frames_allowance": 200.00,
                    "contacts_allowance": 200.00,
                    "frequency": {"exam": "12 months", "lenses": "12 months", "frames": "24 months"}
                },
                "out_of_network": {
                    "exam_reimbursement": 45.00,
                    "frames_reimbursement": 70.00,
                    "contacts_reimbursement": 105.00
                }
            }
        },
        "coverage_effective_dates": {
            "medical": "2025-01-01",
            "dental": "2025-01-01",
            "vision": "2025-02-01"
        }
    })
}

fn make_edi_835_translated() -> Value {
    let mut rng = Rng::new(835);
    let mut claims = Vec::new();

    for i in 0..8 {
        let num_svc = rng.range(1, 6) as usize;
        let mut svcs = Vec::new();

        for s in 0..num_svc {
            let charge = (rng.range_f64(50.0, 500.0) * 100.0).round() / 100.0;
            let paid = (charge * rng.range_f64(0.5, 0.95) * 100.0).round() / 100.0;

            let cas_group = *rng.pick(&["CO", "OA", "PI", "PR"]);
            let cas_reason = rng.range(1, 253);

            svcs.push(json!({
                "svc_procedure_code": format!("{:05}", 99211 + s),
                "svc_charge_amount": charge,
                "svc_paid_amount": paid,
                "svc_revenue_code": format!("{:04}", rng.range(100, 999)),
                "cas_adjustments": [
                    {
                        "group_code": cas_group,
                        "reason_code": cas_reason,
                        "amount": (charge - paid * 100.0).abs().round() / 100.0
                    }
                ]
            }));
        }

        claims.push(json!({
            "claim_id": format!("835-CLM-{:04}", i + 1),
            "patient_name": format!("Patient {}", i + 1),
            "claim_status": if rng.next_f64() < 0.8 { "paid" } else { "denied" },
            "total_charge": svcs.iter().filter_map(|s| s["svc_charge_amount"].as_f64()).sum::<f64>(),
            "total_paid": svcs.iter().filter_map(|s| s["svc_paid_amount"].as_f64()).sum::<f64>(),
            "service_lines": svcs,
            "plb_adjustments": if i < 3 {
                json!([{
                    "provider_id": format!("1{:09}", rng.range(100000000, 999999999)),
                    "fiscal_period": "2025-03",
                    "adjustment_reason": "WO",
                    "amount": (rng.range_f64(10.0, 200.0) * 100.0).round() / 100.0
                }])
            } else {
                json!([])
            }
        }));
    }

    json!({
        "transaction_type": "835",
        "sender": {"name": "BCBS Illinois", "id": "BCBS-IL-001"},
        "receiver": {"name": "Springfield Medical Group", "npi": "1234567890"},
        "payment_date": "2025-03-20",
        "payment_method": "ACH",
        "payment_amount": claims.iter().filter_map(|c| c["total_paid"].as_f64()).sum::<f64>(),
        "claims": claims
    })
}

// ---------------------------------------------------------------------------
// Category 2: API Responses
// ---------------------------------------------------------------------------

fn generate_api(root: &Path) {
    let dir = root.join("api");
    ensure_dir(&dir);

    write_json(&dir.join("paginated_list.json"), &make_paginated_list());
    write_json(&dir.join("nested_api.json"), &make_nested_api());
    write_text(&dir.join("webhook_batch.ndjson"), &make_webhook_batch());
    write_json(&dir.join("schema_drift.json"), &make_schema_drift_v1());
    write_json(&dir.join("schema_drift_v2.json"), &make_schema_drift_v2());
}

fn make_paginated_list() -> Value {
    let mut rng = Rng::new(100);
    let mut items = Vec::new();

    for i in 0..25 {
        let mut item = json!({
            "id": format!("item-{:04}", i + 1),
            "name": format!("Resource {}", i + 1),
            "status": *rng.pick(&["active", "active", "active", "inactive", "pending"]),
            "created_at": format!("2025-{:02}-{:02}T{:02}:{:02}:00Z", rng.range(1, 4), rng.range(1, 29), rng.range(0, 24), rng.range(0, 60)),
            "updated_at": format!("2025-03-{:02}T{:02}:{:02}:00Z", rng.range(1, 29), rng.range(0, 24), rng.range(0, 60)),
            "metadata": {
                "version": rng.range(1, 10),
                "tags": [format!("tag-{}", rng.range(1, 20)), format!("tag-{}", rng.range(1, 20))]
            }
        });

        // Optional fields: some items have description, some have owner
        if i % 3 == 0 {
            item.as_object_mut()
                .unwrap_or_else(|| unreachable!())
                .insert(
                    "description".to_string(),
                    json!(format!("Description for resource {}", i + 1)),
                );
        }
        if i % 4 == 0 {
            item.as_object_mut()
                .unwrap_or_else(|| unreachable!())
                .insert(
                    "owner".to_string(),
                    json!(format!("user-{:03}", rng.range(1, 50))),
                );
        }

        items.push(item);
    }

    json!({
        "data": items,
        "pagination": {
            "total": 142,
            "page": 1,
            "per_page": 25,
            "next_url": "/api/v1/resources?page=2&per_page=25"
        }
    })
}

fn make_nested_api() -> Value {
    json!({
        "user": {
            "id": "usr-001",
            "name": "Alice Thompson",
            "organizations": [
                {
                    "id": "org-001",
                    "name": "Acme Corp",
                    "teams": [
                        {
                            "id": "team-001",
                            "name": "Platform",
                            "projects": [
                                {
                                    "id": "proj-001",
                                    "name": "API Gateway",
                                    "environments": [
                                        {
                                            "id": "env-001",
                                            "name": "production",
                                            "deployments": [
                                                {
                                                    "id": "deploy-001",
                                                    "version": "2.1.0",
                                                    "status": "running",
                                                    "logs": [
                                                        {"timestamp": "2025-03-15T10:00:00Z", "level": "info", "message": "Deployment started"},
                                                        {"timestamp": "2025-03-15T10:01:00Z", "level": "info", "message": "Health check passed"},
                                                        {"timestamp": "2025-03-15T10:02:00Z", "level": "warn", "message": "High memory usage detected"}
                                                    ]
                                                },
                                                {
                                                    "id": "deploy-002",
                                                    "version": "2.0.9",
                                                    "status": "stopped",
                                                    "logs": [
                                                        {"timestamp": "2025-03-14T08:00:00Z", "level": "info", "message": "Deployment stopped"}
                                                    ]
                                                }
                                            ]
                                        },
                                        {
                                            "id": "env-002",
                                            "name": "staging",
                                            "deployments": [
                                                {
                                                    "id": "deploy-003",
                                                    "version": "2.2.0-rc1",
                                                    "status": "running",
                                                    "logs": [
                                                        {"timestamp": "2025-03-15T09:00:00Z", "level": "info", "message": "Canary deployment started"},
                                                        {"timestamp": "2025-03-15T09:05:00Z", "level": "error", "message": "Failed health check on port 8080"}
                                                    ]
                                                }
                                            ]
                                        }
                                    ]
                                },
                                {
                                    "id": "proj-002",
                                    "name": "Auth Service",
                                    "environments": [
                                        {
                                            "id": "env-003",
                                            "name": "production",
                                            "deployments": [
                                                {
                                                    "id": "deploy-004",
                                                    "version": "1.5.2",
                                                    "status": "running",
                                                    "logs": []
                                                }
                                            ]
                                        }
                                    ]
                                }
                            ]
                        },
                        {
                            "id": "team-002",
                            "name": "Data",
                            "projects": [
                                {
                                    "id": "proj-003",
                                    "name": "ETL Pipeline",
                                    "environments": [
                                        {
                                            "id": "env-004",
                                            "name": "production",
                                            "deployments": []
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                }
            ]
        }
    })
}

fn make_webhook_batch() -> String {
    let mut rng = Rng::new(200);
    let mut lines = Vec::with_capacity(100);

    let event_types = [
        "user.created",
        "user.updated",
        "user.deleted",
        "order.placed",
        "order.shipped",
        "order.delivered",
        "payment.completed",
        "payment.failed",
        "payment.refunded",
        "subscription.started",
        "subscription.cancelled",
    ];

    for i in 0..100 {
        let event_type = *rng.pick(&event_types);
        let ts_hour = rng.range(0, 24);
        let ts_min = rng.range(0, 60);
        let ts_sec = rng.range(0, 60);

        let mut event = json!({
            "event_id": format!("evt-{:06}", i + 1),
            "event_type": event_type,
            "timestamp": format!("2025-03-15T{:02}:{:02}:{:02}Z", ts_hour, ts_min, ts_sec),
            "source": "webhook-gateway"
        });

        // Schema varies by event type
        match event_type {
            "user.created" | "user.updated" | "user.deleted" => {
                event
                    .as_object_mut()
                    .unwrap_or_else(|| unreachable!())
                    .insert(
                        "data".to_string(),
                        json!({
                            "user_id": format!("usr-{:05}", rng.range(1, 10000)),
                            "email": format!("user{}@example.com", rng.range(1, 999)),
                            "name": format!("User {}", rng.range(1, 500))
                        }),
                    );
            }
            "order.placed" | "order.shipped" | "order.delivered" => {
                event
                    .as_object_mut()
                    .unwrap_or_else(|| unreachable!())
                    .insert(
                        "data".to_string(),
                        json!({
                            "order_id": format!("ord-{:06}", rng.range(1, 100000)),
                            "user_id": format!("usr-{:05}", rng.range(1, 10000)),
                            "total": (rng.range_f64(10.0, 500.0) * 100.0).round() / 100.0,
                            "items_count": rng.range(1, 10)
                        }),
                    );
            }
            "payment.completed" | "payment.failed" | "payment.refunded" => {
                event
                    .as_object_mut()
                    .unwrap_or_else(|| unreachable!())
                    .insert(
                        "data".to_string(),
                        json!({
                            "payment_id": format!("pay-{:06}", rng.range(1, 100000)),
                            "amount": (rng.range_f64(5.0, 1000.0) * 100.0).round() / 100.0,
                            "currency": *rng.pick(&["USD", "EUR", "GBP"]),
                            "method": *rng.pick(&["card", "bank_transfer", "paypal"])
                        }),
                    );
            }
            _ => {
                event
                    .as_object_mut()
                    .unwrap_or_else(|| unreachable!())
                    .insert(
                        "data".to_string(),
                        json!({
                            "subscription_id": format!("sub-{:05}", rng.range(1, 5000)),
                            "plan": *rng.pick(&["basic", "pro", "enterprise"])
                        }),
                    );
            }
        }

        // Some events have retry metadata
        if rng.next_f64() < 0.15 {
            event.as_object_mut().unwrap_or_else(|| unreachable!()).insert("retry".to_string(), json!({
                "attempt": rng.range(2, 6),
                "original_timestamp": format!("2025-03-14T{:02}:{:02}:{:02}Z", ts_hour, ts_min, ts_sec),
                "reason": "timeout"
            }));
        }

        lines.push(serde_json::to_string(&event).unwrap_or_default());
    }

    lines.join("\n")
}

fn make_schema_drift_v1() -> Value {
    let mut items = Vec::new();
    let mut rng = Rng::new(300);

    for i in 0..20 {
        items.push(json!({
            "id": i + 1,
            "name": format!("Widget {}", rng.range(100, 999)),
            "price": format!("{:.2}", rng.range_f64(10.0, 200.0)),
            "category": *rng.pick(&["electronics", "clothing", "home", "toys"]),
            "in_stock": rng.next_f64() < 0.7,
            "rating": (rng.range_f64(1.0, 5.0) * 10.0).round() / 10.0,
            "legacy_field": "deprecated_value"
        }));
    }

    json!({
        "api_version": "1.0",
        "items": items,
        "metadata": {"generated_at": "2025-01-15T00:00:00Z"}
    })
}

fn make_schema_drift_v2() -> Value {
    let mut items = Vec::new();
    let mut rng = Rng::new(301);

    for i in 0..20 {
        items.push(json!({
            "id": i + 1,
            "name": format!("Widget {}", rng.range(100, 999)),
            // Type change: price is now a number, not a string
            "price": (rng.range_f64(10.0, 200.0) * 100.0).round() / 100.0,
            "category": *rng.pick(&["electronics", "clothing", "home", "toys", "outdoor", "sports"]),
            "in_stock": rng.next_f64() < 0.7,
            "rating": (rng.range_f64(1.0, 5.0) * 10.0).round() / 10.0,
            // legacy_field removed, new field added
            "sku": format!("SKU-{:06}", rng.range(100000, 999999)),
            "warehouse": *rng.pick(&["east", "west", "central"])
        }));
    }

    json!({
        "api_version": "2.0",
        "items": items,
        "metadata": {
            "generated_at": "2025-03-15T00:00:00Z",
            "schema_version": "2.0.0"
        }
    })
}

// ---------------------------------------------------------------------------
// Category 3: Configuration
// ---------------------------------------------------------------------------

fn generate_config(root: &Path) {
    let dir = root.join("config");
    ensure_dir(&dir);

    write_text(&dir.join("kubernetes_deployment.yaml"), &make_k8s_yaml());
    write_json(&dir.join("cloudformation.json"), &make_cloudformation());
    write_json(&dir.join("terraform_state.json"), &make_terraform_state());
}

fn make_k8s_yaml() -> String {
    r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: api-gateway
  namespace: production
  labels:
    app: api-gateway
    version: v2.1.0
    team: platform
spec:
  replicas: 3
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxUnavailable: 1
      maxSurge: 1
  selector:
    matchLabels:
      app: api-gateway
  template:
    metadata:
      labels:
        app: api-gateway
        version: v2.1.0
    spec:
      containers:
        - name: api-gateway
          image: registry.example.com/api-gateway:2.1.0
          ports:
            - containerPort: 8080
              protocol: TCP
          resources:
            requests:
              memory: "256Mi"
              cpu: "250m"
            limits:
              memory: "512Mi"
              cpu: "500m"
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: db-credentials
                  key: url
            - name: LOG_LEVEL
              value: "info"
            - name: METRICS_PORT
              value: "9090"
          volumeMounts:
            - name: config-volume
              mountPath: /etc/config
              readOnly: true
          livenessProbe:
            httpGet:
              path: /healthz
              port: 8080
            initialDelaySeconds: 15
            periodSeconds: 10
          readinessProbe:
            httpGet:
              path: /ready
              port: 8080
            initialDelaySeconds: 5
            periodSeconds: 5
      volumes:
        - name: config-volume
          configMap:
            name: api-gateway-config
---
apiVersion: v1
kind: Service
metadata:
  name: api-gateway-service
  namespace: production
spec:
  selector:
    app: api-gateway
  ports:
    - protocol: TCP
      port: 80
      targetPort: 8080
  type: ClusterIP
---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: api-gateway-ingress
  namespace: production
  annotations:
    nginx.ingress.kubernetes.io/rewrite-target: /
spec:
  rules:
    - host: api.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: api-gateway-service
                port:
                  number: 80
"#
    .to_string()
}

fn make_cloudformation() -> Value {
    json!({
        "AWSTemplateFormatVersion": "2010-09-09",
        "Description": "Production stack with EC2, RDS, S3, and Lambda",
        "Parameters": {
            "Environment": {
                "Type": "String",
                "Default": "production",
                "AllowedValues": ["production", "staging", "development"]
            },
            "InstanceType": {
                "Type": "String",
                "Default": "t3.medium",
                "AllowedValues": ["t3.small", "t3.medium", "t3.large"]
            },
            "DBInstanceClass": {
                "Type": "String",
                "Default": "db.r5.large"
            }
        },
        "Conditions": {
            "IsProduction": {"Fn::Equals": [{"Ref": "Environment"}, "production"]},
            "CreateReadReplica": {"Fn::Equals": [{"Ref": "Environment"}, "production"]}
        },
        "Mappings": {
            "RegionMap": {
                "us-east-1": {"AMI": "ami-0abcdef1234567890"},
                "us-west-2": {"AMI": "ami-0fedcba9876543210"}
            }
        },
        "Resources": {
            "WebServerInstance": {
                "Type": "AWS::EC2::Instance",
                "Properties": {
                    "ImageId": {"Fn::FindInMap": ["RegionMap", {"Ref": "AWS::Region"}, "AMI"]},
                    "InstanceType": {"Ref": "InstanceType"},
                    "SecurityGroupIds": [{"Ref": "WebSecurityGroup"}],
                    "SubnetId": {"Ref": "PublicSubnet"},
                    "Tags": [{"Key": "Name", "Value": {"Fn::Sub": "${Environment}-web-server"}}]
                }
            },
            "WebSecurityGroup": {
                "Type": "AWS::EC2::SecurityGroup",
                "Properties": {
                    "GroupDescription": "Security group for web servers",
                    "VpcId": {"Ref": "VPC"},
                    "SecurityGroupIngress": [
                        {"IpProtocol": "tcp", "FromPort": 80, "ToPort": 80, "CidrIp": "0.0.0.0/0"},
                        {"IpProtocol": "tcp", "FromPort": 443, "ToPort": 443, "CidrIp": "0.0.0.0/0"}
                    ]
                }
            },
            "Database": {
                "Type": "AWS::RDS::DBInstance",
                "Properties": {
                    "DBInstanceClass": {"Ref": "DBInstanceClass"},
                    "Engine": "postgres",
                    "EngineVersion": "15.4",
                    "AllocatedStorage": 100,
                    "StorageType": "gp3",
                    "MasterUsername": "admin",
                    "MasterUserPassword": {"Ref": "DBPassword"},
                    "MultiAZ": {"Fn::If": ["IsProduction", true, false]},
                    "VPCSecurityGroups": [{"Ref": "DBSecurityGroup"}]
                }
            },
            "DataBucket": {
                "Type": "AWS::S3::Bucket",
                "Properties": {
                    "BucketName": {"Fn::Sub": "${Environment}-data-bucket"},
                    "VersioningConfiguration": {"Status": "Enabled"},
                    "EncryptionConfiguration": {
                        "Rules": [{"ApplyServerSideEncryptionByDefault": {"SSEAlgorithm": "aws:kms"}}]
                    }
                }
            },
            "ProcessorFunction": {
                "Type": "AWS::Lambda::Function",
                "Properties": {
                    "FunctionName": {"Fn::Sub": "${Environment}-processor"},
                    "Runtime": "python3.11",
                    "Handler": "index.handler",
                    "MemorySize": 256,
                    "Timeout": 300,
                    "Environment": {
                        "Variables": {
                            "BUCKET_NAME": {"Ref": "DataBucket"},
                            "DB_HOST": {"Fn::GetAtt": ["Database", "Endpoint.Address"]}
                        }
                    }
                }
            }
        }
    })
}

fn make_terraform_state() -> Value {
    json!({
        "version": 4,
        "terraform_version": "1.7.0",
        "serial": 42,
        "lineage": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
        "outputs": {
            "vpc_id": {"value": "vpc-0abcdef1234567890", "type": "string"},
            "public_subnets": {
                "value": ["subnet-0aaa", "subnet-0bbb", "subnet-0ccc"],
                "type": ["list", "string"]
            }
        },
        "resources": [
            {
                "module": "module.networking",
                "mode": "managed",
                "type": "aws_vpc",
                "name": "main",
                "provider": "provider[\"registry.terraform.io/hashicorp/aws\"]",
                "instances": [{
                    "attributes": {
                        "id": "vpc-0abcdef1234567890",
                        "cidr_block": "10.0.0.0/16",
                        "enable_dns_hostnames": true,
                        "enable_dns_support": true,
                        "tags": {"Name": "production-vpc", "Environment": "production"}
                    },
                    "dependencies": []
                }]
            },
            {
                "module": "module.networking",
                "mode": "managed",
                "type": "aws_subnet",
                "name": "public",
                "provider": "provider[\"registry.terraform.io/hashicorp/aws\"]",
                "instances": [
                    {
                        "index_key": 0,
                        "attributes": {
                            "id": "subnet-0aaa",
                            "vpc_id": "vpc-0abcdef1234567890",
                            "cidr_block": "10.0.1.0/24",
                            "availability_zone": "us-east-1a",
                            "map_public_ip_on_launch": true
                        },
                        "dependencies": ["module.networking.aws_vpc.main"]
                    },
                    {
                        "index_key": 1,
                        "attributes": {
                            "id": "subnet-0bbb",
                            "vpc_id": "vpc-0abcdef1234567890",
                            "cidr_block": "10.0.2.0/24",
                            "availability_zone": "us-east-1b",
                            "map_public_ip_on_launch": true
                        },
                        "dependencies": ["module.networking.aws_vpc.main"]
                    }
                ]
            },
            {
                "module": "module.compute",
                "mode": "managed",
                "type": "aws_instance",
                "name": "web",
                "provider": "provider[\"registry.terraform.io/hashicorp/aws\"]",
                "instances": [{
                    "attributes": {
                        "id": "i-0abcdef123456",
                        "ami": "ami-0abcdef1234567890",
                        "instance_type": "t3.medium",
                        "subnet_id": "subnet-0aaa",
                        "security_groups": ["sg-0xyz"],
                        "tags": {"Name": "web-server", "Environment": "production"}
                    },
                    "dependencies": [
                        "module.networking.aws_subnet.public",
                        "module.networking.aws_vpc.main"
                    ]
                }]
            },
            {
                "module": "module.database",
                "mode": "managed",
                "type": "aws_db_instance",
                "name": "primary",
                "provider": "provider[\"registry.terraform.io/hashicorp/aws\"]",
                "instances": [{
                    "attributes": {
                        "id": "mydb-instance",
                        "engine": "postgres",
                        "engine_version": "15.4",
                        "instance_class": "db.r5.large",
                        "allocated_storage": 100,
                        "multi_az": true,
                        "endpoint": "mydb.cluster.us-east-1.rds.amazonaws.com:5432"
                    },
                    "dependencies": [
                        "module.networking.aws_subnet.public",
                        "module.networking.aws_vpc.main"
                    ]
                }]
            }
        ]
    })
}

// ---------------------------------------------------------------------------
// Category 4: Analytics/Telemetry
// ---------------------------------------------------------------------------

fn generate_telemetry(root: &Path) {
    let dir = root.join("telemetry");
    ensure_dir(&dir);

    write_text(&dir.join("event_stream.ndjson"), &make_event_stream());
    write_text(&dir.join("metrics.csv"), &make_metrics_csv());
}

fn make_event_stream() -> String {
    let mut rng = Rng::new(400);
    let mut lines = Vec::with_capacity(200);

    let event_types = [
        "page_view",
        "click",
        "purchase",
        "error",
        "scroll",
        "form_submit",
    ];
    let pages = [
        "/home",
        "/products",
        "/cart",
        "/checkout",
        "/profile",
        "/search",
    ];

    for i in 0..200 {
        let event_type = *rng.pick(&event_types);
        let session_id = format!("sess-{:04}", rng.range(1, 50));
        let user_id = format!("user-{:04}", rng.range(1, 200));

        // Temporal patterns: burst at hours 10-14, gap at 3-5
        let hour = if i < 50 {
            rng.range(10, 15)
        } else if i < 60 {
            rng.range(3, 6)
        } else {
            rng.range(0, 24)
        };

        let mut event = json!({
            "event_type": event_type,
            "timestamp": format!("2025-03-15T{:02}:{:02}:{:02}.{:03}Z", hour, rng.range(0, 60), rng.range(0, 60), rng.range(0, 1000)),
            "session_id": session_id,
            "user_id": user_id,
            "page": *rng.pick(&pages)
        });

        // Type-specific payload
        match event_type {
            "click" => {
                event
                    .as_object_mut()
                    .unwrap_or_else(|| unreachable!())
                    .insert(
                        "element".to_string(),
                        json!(*rng.pick(&["button", "link", "image", "nav"])),
                    );
                event
                    .as_object_mut()
                    .unwrap_or_else(|| unreachable!())
                    .insert("x".to_string(), json!(rng.range(0, 1920)));
                event
                    .as_object_mut()
                    .unwrap_or_else(|| unreachable!())
                    .insert("y".to_string(), json!(rng.range(0, 1080)));
            }
            "purchase" => {
                event
                    .as_object_mut()
                    .unwrap_or_else(|| unreachable!())
                    .insert(
                        "amount".to_string(),
                        json!((rng.range_f64(5.0, 500.0) * 100.0).round() / 100.0),
                    );
                event
                    .as_object_mut()
                    .unwrap_or_else(|| unreachable!())
                    .insert("items".to_string(), json!(rng.range(1, 8)));
            }
            "error" => {
                event
                    .as_object_mut()
                    .unwrap_or_else(|| unreachable!())
                    .insert(
                        "error_code".to_string(),
                        json!(*rng.pick(&["500", "502", "503", "timeout"])),
                    );
                event
                    .as_object_mut()
                    .unwrap_or_else(|| unreachable!())
                    .insert("stack_trace".to_string(), json!("at main.js:42"));
            }
            _ => {}
        }

        // Schema variation: some events have debug fields
        if rng.next_f64() < 0.1 {
            event
                .as_object_mut()
                .unwrap_or_else(|| unreachable!())
                .insert(
                    "debug".to_string(),
                    json!({
                        "render_time_ms": rng.range(10, 5000),
                        "memory_mb": rng.range(50, 500)
                    }),
                );
        }

        lines.push(serde_json::to_string(&event).unwrap_or_default());
    }

    lines.join("\n")
}

fn make_metrics_csv() -> String {
    let mut rng = Rng::new(500);
    let mut csv = String::from(
        "timestamp,host,cpu_pct,memory_pct,disk_io,network_in,network_out,error_count\n",
    );

    let hosts = ["web-01", "web-02", "web-03", "db-01", "worker-01"];

    for row in 0..500 {
        let host_idx = row % 5;
        let host = hosts[host_idx];
        let hour = row / 5 / 12; // 100 time slots, ~8 hours
        let minute = (row / 5 % 12) * 5;
        let ts = format!("2025-03-15T{:02}:{:02}:00Z", hour.min(23), minute.min(59));

        // Anomalous CPU spikes on web-01
        let cpu = if host == "web-01" && (row / 5 > 40 && row / 5 < 50) {
            rng.range_f64(90.0, 100.0)
        } else {
            rng.range_f64(10.0, 70.0)
        };

        // worker-01 goes offline (missing values)
        if host == "worker-01" && row / 5 > 60 && row / 5 < 80 {
            let _ = writeln!(csv, "{ts},{host},,,,,,");
            continue;
        }

        let mem = rng.range_f64(30.0, 85.0);
        let disk = rng.range(100, 50000);
        let net_in = rng.range(1000, 500000);
        let net_out = rng.range(500, 300000);
        let errors = if rng.next_f64() < 0.05 {
            rng.range(1, 20)
        } else {
            0
        };

        let _ = writeln!(
            csv,
            "{ts},{host},{:.1},{:.1},{disk},{net_in},{net_out},{errors}",
            cpu, mem
        );
    }

    csv
}

// ---------------------------------------------------------------------------
// Category 5: Financial
// ---------------------------------------------------------------------------

fn generate_financial(root: &Path) {
    let dir = root.join("financial");
    ensure_dir(&dir);

    write_text(&dir.join("transactions.ndjson"), &make_transactions());
    write_json(&dir.join("invoice.json"), &make_invoice());
}

fn make_transactions() -> String {
    let mut rng = Rng::new(600);
    let mut lines = Vec::with_capacity(100);

    let currencies = ["USD", "EUR", "GBP", "JPY", "CAD"];
    let merchants = [
        "Amazon",
        "Walmart",
        "Target",
        "Starbucks",
        "Shell Gas",
        "Netflix",
        "Uber",
        "Whole Foods",
        "CVS Pharmacy",
        "Home Depot",
    ];
    let categories = [
        "retail",
        "food",
        "gas",
        "entertainment",
        "services",
        "transfer",
    ];

    for i in 0..100 {
        // Benford's Law amounts (mostly), with some synthetic outliers
        let amount = if i == 42 || i == 43 || i == 44 {
            // Rapid small transactions (suspicious pattern)
            (rng.range_f64(0.50, 2.0) * 100.0).round() / 100.0
        } else if i == 77 || i == 78 {
            // Round-number amounts (suspicious)
            *rng.pick(&[100.0, 500.0, 1000.0, 2000.0])
        } else if i == 90 {
            // Large outlier
            15000.00
        } else {
            rng.benford_amount()
        };

        let currency = *rng.pick(&currencies);
        let merchant = *rng.pick(&merchants);

        let txn = json!({
            "transaction_id": format!("TXN-{:08}", i + 10000001),
            "account_id": format!("ACCT-{:06}", rng.range(100000, 200000)),
            "timestamp": format!("2025-03-{:02}T{:02}:{:02}:{:02}Z",
                rng.range(1, 29), rng.range(0, 24), rng.range(0, 60), rng.range(0, 60)),
            "amount": amount,
            "currency": currency,
            "merchant": {
                "name": merchant,
                "id": format!("MERCH-{:05}", rng.range(10000, 99999)),
                "category": *rng.pick(&categories)
            },
            "type": *rng.pick(&["debit", "credit"]),
            "status": *rng.pick(&["completed", "completed", "completed", "pending", "declined"])
        });

        lines.push(serde_json::to_string(&txn).unwrap_or_default());
    }

    lines.join("\n")
}

fn make_invoice() -> Value {
    json!({
        "invoice_id": "INV-2025-003456",
        "issue_date": "2025-03-01",
        "due_date": "2025-04-01",
        "payment_terms": "Net 30",
        "vendor": {
            "name": "Acme Supply Co.",
            "address": {
                "street": "789 Industrial Pkwy",
                "city": "Chicago",
                "state": "IL",
                "zip": "60601",
                "country": "US"
            },
            "tax_id": "12-3456789",
            "contact": {
                "name": "Bob Builder",
                "email": "bob@acmesupply.com",
                "phone": "+1-312-555-0100"
            }
        },
        "buyer": {
            "name": "Springfield Hospital",
            "address": {
                "street": "100 Hospital Drive",
                "city": "Springfield",
                "state": "IL",
                "zip": "62701",
                "country": "US"
            },
            "purchase_order": "PO-2025-1234"
        },
        "line_items": [
            {
                "line": 1,
                "description": "Surgical gloves (box of 100)",
                "sku": "SG-100-LG",
                "quantity": 50,
                "unit_price": 12.99,
                "discount_pct": 5.0,
                "subtotal": 617.03,
                "tax_rate": 0.0825,
                "tax_amount": 50.91,
                "total": 667.93
            },
            {
                "line": 2,
                "description": "Disposable face masks (box of 50)",
                "sku": "FM-50-BLU",
                "quantity": 200,
                "unit_price": 8.49,
                "discount_pct": 10.0,
                "subtotal": 1528.20,
                "tax_rate": 0.0825,
                "tax_amount": 126.08,
                "total": 1654.28
            },
            {
                "line": 3,
                "description": "Hand sanitizer (gallon)",
                "sku": "HS-GAL-01",
                "quantity": 30,
                "unit_price": 24.99,
                "discount_pct": 0.0,
                "subtotal": 749.70,
                "tax_rate": 0.0825,
                "tax_amount": 61.85,
                "total": 811.55
            },
            {
                "line": 4,
                "description": "Medical tape rolls",
                "sku": "MT-1IN-12PK",
                "quantity": 100,
                "unit_price": 3.75,
                "discount_pct": 0.0,
                "subtotal": 375.00,
                "tax_rate": 0.0825,
                "tax_amount": 30.94,
                "total": 405.94
            }
        ],
        "subtotal": 3269.93,
        "total_discount": 161.18,
        "total_tax": 269.78,
        "grand_total": 3539.70,
        "payment_instructions": {
            "bank_name": "First National Bank",
            "routing_number": "071000013",
            "account_number": "1234567890",
            "reference": "INV-2025-003456"
        },
        "notes": "Please reference PO number on remittance."
    })
}

// ---------------------------------------------------------------------------
// Category 6: Edge Cases
// ---------------------------------------------------------------------------

fn generate_edge_cases(root: &Path) {
    let dir = root.join("edge_cases");
    ensure_dir(&dir);

    write_json(&dir.join("empty_object.json"), &json!({}));
    write_json(&dir.join("empty_array.json"), &json!([]));
    write_text(&dir.join("scalar_string.json"), "\"hello\"");
    write_text(&dir.join("scalar_number.json"), "42");
    write_text(&dir.join("scalar_null.json"), "null");

    // Deeply nested: 100 levels
    let mut nested = json!("leaf");
    for i in (0..100).rev() {
        let mut m = serde_json::Map::new();
        m.insert(format!("level_{}", i), nested);
        nested = Value::Object(m);
    }
    write_json(&dir.join("deeply_nested.json"), &nested);

    // Wide object: 1000 keys
    let mut wide = serde_json::Map::new();
    for i in 0..1000 {
        wide.insert(format!("key_{:04}", i), json!(i));
    }
    write_json(&dir.join("wide_object.json"), &Value::Object(wide));

    // Big array: 5000 identical elements
    let big_array: Vec<Value> = (0..5000)
        .map(|_| json!({"value": 1, "label": "x"}))
        .collect();
    write_json(&dir.join("big_array.json"), &Value::Array(big_array));

    // Type chaos
    write_json(
        &dir.join("type_chaos.json"),
        &json!({
            "a_string": "hello",
            "a_number": 42,
            "a_float": 3.14,
            "a_bool": true,
            "a_null": null,
            "an_array": [1, "two", 3.0, true, null, {"nested": "obj"}, [1, 2]],
            "an_object": {"x": 1},
            "an_empty_array": [],
            "an_empty_object": {},
            "a_negative": -99,
            "a_big_number": 9007199254740992_i64,
            "a_small_float": 0.000001
        }),
    );

    // Unicode stress
    write_json(
        &dir.join("unicode_stress.json"),
        &json!({
            "emoji_key_\u{1F600}": "grinning face",
            "cjk_\u{4E16}\u{754C}": "\u{4F60}\u{597D}\u{4E16}\u{754C}",
            "arabic_\u{0627}\u{0644}\u{0639}\u{0627}\u{0644}\u{0645}": "\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}",
            "combining": "e\u{0301}e\u{0308}",
            "zero_width": "hello\u{200B}world\u{200C}test\u{200D}join\u{FEFF}bom",
            "surrogate_safe": "\u{1F4A9}\u{1F680}\u{1F30D}",
            "rtl_mixed": "\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064A}\u{0629} and English",
            "fullwidth": "\u{FF21}\u{FF22}\u{FF23}",
            "normal_key": "normal_value"
        }),
    );

    // All nulls
    write_json(
        &dir.join("all_nulls.json"),
        &json!({
            "a": null, "b": null, "c": null, "d": null, "e": null,
            "nested": {"f": null, "g": null},
            "array_of_nulls": [null, null, null]
        }),
    );

    // Mixed schemas NDJSON
    let mixed = [
        json!({"name": "Alice", "age": 30}),
        json!({"id": 1, "values": [1, 2, 3]}),
        json!({"status": "ok", "code": 200, "message": "success"}),
        json!(["just", "an", "array"]),
        json!({"deeply": {"nested": {"value": true}}}),
        json!({"key": null}),
        json!({"x": 1.5, "y": 2.5, "z": 3.5}),
        json!({"tags": ["a", "b"], "metadata": {"created": "2025-01-01"}}),
        json!(42),
        json!({"empty_containers": {"obj": {}, "arr": []}}),
    ];
    let mixed_lines: Vec<String> = mixed
        .iter()
        .map(|v| serde_json::to_string(v).unwrap_or_default())
        .collect();
    write_text(&dir.join("mixed_schemas.ndjson"), &mixed_lines.join("\n"));

    // Sensitive data (for redaction testing)
    write_json(
        &dir.join("sensitive_data.json"),
        &json!({
            "patient": {
                "name": "Jane Doe",
                "ssn": "123-45-6789",
                "email": "jane.doe@example.com",
                "phone": "+1-555-867-5309",
                "credit_card": "4111-1111-1111-1111",
                "date_of_birth": "1985-06-12"
            },
            "notes": "Patient SSN is 987-65-4321 and card ending 4242",
            "records": [
                {"ssn": "111-22-3333", "name": "John Smith"},
                {"email": "bob@test.org", "phone": "555-123-4567"}
            ]
        }),
    );
}

// ---------------------------------------------------------------------------
// Category 7: Documents
// ---------------------------------------------------------------------------

fn generate_documents(root: &Path) {
    let dir = root.join("documents");
    ensure_dir(&dir);

    write_text(&dir.join("technical_report.md"), &make_technical_report());
    write_text(&dir.join("readme_example.md"), &make_readme_example());
}

fn make_technical_report() -> String {
    r#"# Q1 2025 Infrastructure Performance Report

## Executive Summary

This report covers the performance metrics for the production infrastructure
during Q1 2025. Overall system availability was **99.97%**, exceeding our
target of 99.95%.

## Key Metrics

| Metric | January | February | March | Target |
|--------|---------|----------|-------|--------|
| Uptime | 99.98% | 99.95% | 99.99% | 99.95% |
| P50 Latency | 42ms | 45ms | 38ms | <50ms |
| P99 Latency | 210ms | 235ms | 190ms | <300ms |
| Error Rate | 0.02% | 0.03% | 0.01% | <0.05% |
| Throughput | 12.5k rps | 13.2k rps | 14.1k rps | >10k rps |

## Infrastructure Changes

### Database Migration

We completed the migration from PostgreSQL 14 to PostgreSQL 15:

```sql
-- Example of the partition scheme used
CREATE TABLE events (
    id BIGSERIAL PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    event_type VARCHAR(50) NOT NULL,
    payload JSONB
) PARTITION BY RANGE (created_at);
```

### Container Orchestration

Migrated from Docker Swarm to Kubernetes:

1. **Week 1-2**: Converted all Dockerfiles and docker-compose configs
2. **Week 3**: Deployed to staging environment
3. **Week 4**: Canary deployment to production (10% traffic)
4. **Week 5-6**: Full production rollout

### Cost Optimization

- Switched to ARM-based instances: **23% cost reduction**
- Implemented spot instances for batch workloads: **40% savings**
- Right-sized database instances: **$2,400/month saved**

## Incident Summary

We had two notable incidents:

- **2025-02-14**: Database connection pool exhaustion (P1, 12 min TTR)
- **2025-03-01**: Certificate expiration on internal API (P2, 35 min TTR)

## Recommendations

> **Priority 1**: Implement automated certificate rotation
>
> **Priority 2**: Add connection pool monitoring alerts
>
> **Priority 3**: Evaluate CDN for static assets

## Appendix

For detailed metrics, see the [Grafana dashboard](https://grafana.internal/d/prod-overview).

---

*Report generated by the SRE team. Contact: sre@example.com*
"#
    .to_string()
}

fn make_readme_example() -> String {
    r#"# Vajra

**Break noise. Preserve truth.**

A high-performance Rust CLI that analyzes arbitrary JSON, extracts structural
and semantic signal, detects anomalies and drift, and emits compact
deterministic essences.

## Installation

```bash
cargo install vajra-cli
```

Or build from source:

```bash
git clone https://github.com/example/vajra.git
cd vajra
cargo build --release
```

## Quick Start

```bash
# Inspect a JSON file
vajra inspect data.json

# Statistical summary
vajra stats data.json

# Detect anomalies
vajra anomalies data.json

# Compare two versions
vajra drift v1.json v2.json
```

## Features

- **Zero-config analysis**: Auto-detects structure and schema
- **Multiple output formats**: Text, JSON, Markdown, Compact AI
- **Anomaly detection**: Numeric outliers, rare values, type instability
- **Schema drift**: Structural and distributional comparison
- **Domain plugins**: Medical coding (ICD-10, CPT, NPI, NDC)

## License

MIT OR Apache-2.0
"#
    .to_string()
}

// ---------------------------------------------------------------------------
// Phase 2: Capture golden output
// ---------------------------------------------------------------------------

fn capture_all_golden(root: &Path) {
    let golden_dir = root.join("golden");
    ensure_dir(&golden_dir);

    // Collect all corpus files (excluding golden dir itself)
    let corpus_files = collect_corpus_files(root);

    eprintln!("  Found {} corpus files to analyze", corpus_files.len());

    for file in &corpus_files {
        capture_golden_for_file(file, root, &golden_dir);
    }

    // Schema drift pair
    capture_drift_golden(root, &golden_dir);
}

fn collect_corpus_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(root, &mut files);
    files.sort();
    files
}

fn collect_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();

        // Skip the golden output directory
        if path.file_name().and_then(|n| n.to_str()) == Some("golden") {
            continue;
        }

        if path.is_dir() {
            collect_files_recursive(&path, files);
        } else {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            match ext {
                "json" | "ndjson" | "yaml" | "yml" | "csv" | "md" => {
                    files.push(path);
                }
                _ => {}
            }
        }
    }
}

fn capture_golden_for_file(file: &Path, corpus_root: &Path, golden_dir: &Path) {
    // Determine relative path from corpus root for organizing output
    let rel = file.strip_prefix(corpus_root).unwrap_or(file);
    let stem = rel.to_string_lossy().replace(['/', '\\'], "__");

    eprintln!("  analyzing: {}", rel.display());

    // Load documents
    let docs = match parse_file_auto(file, None) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("    SKIP (parse error): {e}");
            // Write error as golden for this file
            let error_golden = json!({"error": format!("{e}"), "file": rel.to_string_lossy()});
            let out_path = golden_dir.join(format!("{stem}.error.golden.json"));
            write_json(&out_path, &error_golden);
            return;
        }
    };

    if docs.is_empty() {
        eprintln!("    SKIP (no documents)");
        return;
    }

    // Analyze first document
    let doc = &docs[0];

    // 1. Inspect golden (metadata + paths + fingerprints)
    capture_inspect_golden(doc, &stem, golden_dir);

    // 2. Stats golden
    capture_stats_golden(doc, &stem, golden_dir);

    // 3. Fingerprint golden
    capture_fingerprint_golden(doc, &stem, golden_dir);

    // 4. Anomalies golden
    capture_anomalies_golden(doc, &stem, golden_dir);

    // 5. Essence golden
    capture_essence_golden(doc, &stem, golden_dir);
}

fn hex(bytes: &[u8; 32]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn capture_inspect_golden(doc: &Document, stem: &str, golden_dir: &Path) {
    let meta = doc.metadata();
    let all_paths = doc.trie().all_paths();
    let mut path_views = Vec::new();

    for wp in &all_paths {
        if let Some(node) = doc.trie().get(wp) {
            let m = &node.metadata;
            path_views.push(json!({
                "path": wp.to_string(),
                "dominant_type": m.dominant_type().map_or("unknown".to_string(), |t| t.to_string()),
                "count": m.count,
                "type_instability": m.type_instability(),
                "null_rate": m.null_rate(),
            }));
        }
    }

    let fp = match FingerprintAnalyzer.analyze(doc) {
        Ok(fp) => json!({
            "path_set": hex(&fp.path_set),
            "typed_path": hex(&fp.typed_path),
            "shape": hex(&fp.shape),
        }),
        Err(e) => json!({"error": format!("{e}")}),
    };

    let output = json!({
        "metadata": {
            "total_nodes": meta.total_nodes,
            "max_depth": meta.max_depth,
            "distinct_paths": meta.distinct_paths,
            "raw_size_bytes": meta.raw_size_bytes,
        },
        "paths": path_views,
        "fingerprints": fp,
    });

    let out_path = golden_dir.join(format!("{stem}.inspect.golden.json"));
    write_json(&out_path, &output);
}

fn capture_stats_golden(doc: &Document, stem: &str, golden_dir: &Path) {
    let result = match StatsAnalyzer.analyze(doc) {
        Ok(r) => r,
        Err(e) => {
            let out_path = golden_dir.join(format!("{stem}.stats.golden.json"));
            write_json(&out_path, &json!({"error": format!("{e}")}));
            return;
        }
    };

    let mut paths_json = Vec::new();
    for (wp, stats) in &result.paths {
        let numeric = stats.numeric_stats.as_ref().map(|ns| {
            json!({
                "min": ns.min,
                "max": ns.max,
                "mean": ns.mean,
                "median": ns.median,
                "mad": ns.mad,
                "p05": ns.p05,
                "p25": ns.p25,
                "p75": ns.p75,
                "p95": ns.p95,
            })
        });

        let top_values: Vec<Value> = stats
            .top_values
            .iter()
            .map(|(v, c)| {
                json!({
                    "value": v,
                    "count": c,
                })
            })
            .collect();

        paths_json.push(json!({
            "path": wp.to_string(),
            "entropy": stats.entropy,
            "normalized_entropy": stats.normalized_entropy,
            "cardinality": stats.cardinality,
            "total_count": stats.total_count,
            "max_rarity": stats.max_rarity,
            "numeric": numeric,
            "top_values": top_values,
        }));
    }

    let output = json!({"paths": paths_json});
    let out_path = golden_dir.join(format!("{stem}.stats.golden.json"));
    write_json(&out_path, &output);
}

fn capture_fingerprint_golden(doc: &Document, stem: &str, golden_dir: &Path) {
    let result = match FingerprintAnalyzer.analyze(doc) {
        Ok(r) => r,
        Err(e) => {
            let out_path = golden_dir.join(format!("{stem}.fingerprint.golden.json"));
            write_json(&out_path, &json!({"error": format!("{e}")}));
            return;
        }
    };

    let mut motifs: Vec<Value> = result
        .subtree_frequencies
        .iter()
        .filter(|(_, &count)| count > 1)
        .map(|(h, &count)| json!({"hash": hex(h), "count": count}))
        .collect();
    motifs.sort_by(|a, b| {
        b["count"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["count"].as_u64().unwrap_or(0))
    });

    let output = json!({
        "path_set": hex(&result.path_set),
        "typed_path": hex(&result.typed_path),
        "shape": hex(&result.shape),
        "repeated_motifs": motifs,
    });

    let out_path = golden_dir.join(format!("{stem}.fingerprint.golden.json"));
    write_json(&out_path, &output);
}

fn capture_anomalies_golden(doc: &Document, stem: &str, golden_dir: &Path) {
    let analyzer = AnomalyAnalyzer::default();
    let report = match analyzer.analyze(doc) {
        Ok(r) => r,
        Err(e) => {
            let out_path = golden_dir.join(format!("{stem}.anomalies.golden.json"));
            write_json(&out_path, &json!({"error": format!("{e}")}));
            return;
        }
    };

    let instabilities: Vec<Value> = report
        .type_instabilities
        .iter()
        .map(|ti| {
            json!({
                "path": ti.path,
                "instability": ti.instability,
                "dominant_type": ti.dominant_type,
                "type_distribution": ti.type_distribution,
            })
        })
        .collect();

    let outliers: Vec<Value> = report
        .numeric_outliers
        .iter()
        .map(|no| {
            json!({
                "path": no.path,
                "value": no.value,
                "z_mad": no.z_mad,
                "median": no.median,
                "mad": no.mad,
            })
        })
        .collect();

    let rare: Vec<Value> = report
        .rare_values
        .iter()
        .map(|rv| {
            json!({
                "value": rv.value,
                "count": rv.count,
                "rarity_bits": rv.rarity_bits,
            })
        })
        .collect();

    let output = json!({
        "type_instabilities": instabilities,
        "numeric_outliers": outliers,
        "rare_values": rare,
    });

    let out_path = golden_dir.join(format!("{stem}.anomalies.golden.json"));
    write_json(&out_path, &output);
}

fn capture_essence_golden(doc: &Document, stem: &str, golden_dir: &Path) {
    let profile = EngineerProfile;

    let stats = match StatsAnalyzer.analyze(doc) {
        Ok(r) => r,
        Err(_) => {
            let out_path = golden_dir.join(format!("{stem}.essence.golden.json"));
            write_json(&out_path, &json!({"error": "stats analysis failed"}));
            return;
        }
    };

    let anomalies = AnomalyAnalyzer::default().analyze(doc).ok();
    let fingerprint = FingerprintAnalyzer.analyze(doc).ok();

    let mut builder = EssenceBuilder::new(doc, &profile).with_stats(&stats);

    if let Some(ref a) = anomalies {
        builder = builder.with_anomalies(a);
    }
    if let Some(ref f) = fingerprint {
        builder = builder.with_fingerprint(f);
    }

    let essence = builder.build();

    let rendered = match profile.render(&essence, OutputFormat::Json) {
        Ok(r) => r,
        Err(e) => {
            let out_path = golden_dir.join(format!("{stem}.essence.golden.json"));
            write_json(&out_path, &json!({"error": format!("{e}")}));
            return;
        }
    };

    // Parse the rendered JSON to store as proper JSON (not string-escaped)
    let parsed: Value =
        serde_json::from_str(&rendered).unwrap_or_else(|_| json!({"rendered": rendered}));
    let out_path = golden_dir.join(format!("{stem}.essence.golden.json"));
    write_json(&out_path, &parsed);
}

fn capture_drift_golden(corpus_root: &Path, golden_dir: &Path) {
    let v1_path = corpus_root.join("api").join("schema_drift.json");
    let v2_path = corpus_root.join("api").join("schema_drift_v2.json");

    if !v1_path.exists() || !v2_path.exists() {
        eprintln!("  SKIP drift: schema_drift files not found");
        return;
    }

    eprintln!("  analyzing drift: schema_drift.json <-> schema_drift_v2.json");

    let v1_docs = match parse_file_auto(&v1_path, None) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("    SKIP (parse error v1): {e}");
            return;
        }
    };
    let v2_docs = match parse_file_auto(&v2_path, None) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("    SKIP (parse error v2): {e}");
            return;
        }
    };

    if v1_docs.is_empty() || v2_docs.is_empty() {
        eprintln!("    SKIP (no documents)");
        return;
    }

    let report = full_drift(&v1_docs[0], &v2_docs[0]);

    let output = json!({
        "structural_similarity": report.structural_similarity,
        "severity": format!("{:?}", report.severity),
        "added_paths": report.path_diff.added.iter().map(|p| p.to_string()).collect::<Vec<_>>(),
        "removed_paths": report.path_diff.removed.iter().map(|p| p.to_string()).collect::<Vec<_>>(),
        "type_changes": report.type_changes.iter().map(|tc| json!({
            "path": tc.path,
            "from": tc.from,
            "to": tc.to,
        })).collect::<Vec<_>>(),
        "distributional_drifts": report.distributional_drifts.iter().map(|dd| json!({
            "path": dd.path,
            "metric": format!("{:?}", dd.metric),
            "value": dd.value,
        })).collect::<Vec<_>>(),
    });

    let out_path = golden_dir.join("drift.golden.json");
    write_json(&out_path, &output);
}
