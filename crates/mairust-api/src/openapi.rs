//! OpenAPI documentation
//!
//! Provides OpenAPI 3.0 specification and Swagger UI for the MaiRust API.

use axum::{
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use serde_json::json;

/// Create OpenAPI routes
pub fn create_openapi_routes() -> Router {
    Router::new()
        .route("/openapi.json", get(openapi_json))
        .route("/docs", get(swagger_ui))
}

/// OpenAPI JSON specification endpoint
async fn openapi_json() -> impl IntoResponse {
    Json(get_openapi_spec())
}

/// Swagger UI HTML endpoint
async fn swagger_ui() -> impl IntoResponse {
    Html(SWAGGER_UI_HTML)
}

/// Get the OpenAPI specification as JSON
fn get_openapi_spec() -> serde_json::Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "MaiRust API",
            "description": "REST API for MaiRust Mail Server\n\n## Authentication\n\nAll API endpoints (except health checks) require authentication via API key.\n\n- **Header**: `X-API-Key: <your-api-key>`\n- **Bearer**: `Authorization: Bearer <your-api-key>`",
            "version": "1.0.0",
            "contact": {
                "name": "MaiRust Team",
                "url": "https://github.com/example/mairust"
            },
            "license": {
                "name": "Apache-2.0",
                "url": "https://www.apache.org/licenses/LICENSE-2.0"
            }
        },
        "servers": [
            {
                "url": "/api/v1",
                "description": "API v1"
            }
        ],
        "tags": [
            {"name": "health", "description": "Health check endpoints"},
            {"name": "tenants", "description": "Tenant management (admin only)"},
            {"name": "users", "description": "User management"},
            {"name": "domains", "description": "Domain management"},
            {"name": "mailboxes", "description": "Mailbox management"},
            {"name": "messages", "description": "Message operations"},
            {"name": "hooks", "description": "Hook/plugin management"},
            {"name": "send", "description": "Email sending"}
        ],
        "paths": {
            // Health endpoints
            "/health": {
                "get": {
                    "tags": ["health"],
                    "summary": "Basic health check",
                    "operationId": "health",
                    "responses": {
                        "200": {
                            "description": "Service is healthy",
                            "content": {
                                "application/json": {
                                    "schema": {"$ref": "#/components/schemas/HealthResponse"}
                                }
                            }
                        }
                    }
                }
            },
            "/health/live": {
                "get": {
                    "tags": ["health"],
                    "summary": "Liveness probe",
                    "operationId": "liveness",
                    "responses": {
                        "200": {"description": "Service is alive"},
                        "503": {"description": "Service is not alive"}
                    }
                }
            },
            "/health/ready": {
                "get": {
                    "tags": ["health"],
                    "summary": "Readiness probe",
                    "operationId": "readiness",
                    "responses": {
                        "200": {"description": "Service is ready"},
                        "503": {"description": "Service is not ready"}
                    }
                }
            },
            "/health/detailed": {
                "get": {
                    "tags": ["health"],
                    "summary": "Detailed health check",
                    "operationId": "healthDetailed",
                    "responses": {
                        "200": {
                            "description": "Detailed health status",
                            "content": {
                                "application/json": {
                                    "schema": {"$ref": "#/components/schemas/DetailedHealthResponse"}
                                }
                            }
                        }
                    }
                }
            },
            // Tenant endpoints
            "/admin/tenants": {
                "get": {
                    "tags": ["tenants"],
                    "summary": "List all tenants",
                    "operationId": "listTenants",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "responses": {
                        "200": {
                            "description": "List of tenants",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": {"$ref": "#/components/schemas/Tenant"}
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "tags": ["tenants"],
                    "summary": "Create a new tenant",
                    "operationId": "createTenant",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/CreateTenantRequest"}
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Tenant created",
                            "content": {
                                "application/json": {
                                    "schema": {"$ref": "#/components/schemas/Tenant"}
                                }
                            }
                        }
                    }
                }
            },
            "/admin/tenants/{id}": {
                "get": {
                    "tags": ["tenants"],
                    "summary": "Get a tenant by ID",
                    "operationId": "getTenant",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "responses": {
                        "200": {"description": "Tenant details"},
                        "404": {"description": "Tenant not found"}
                    }
                },
                "delete": {
                    "tags": ["tenants"],
                    "summary": "Delete a tenant",
                    "operationId": "deleteTenant",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "responses": {
                        "204": {"description": "Tenant deleted"},
                        "404": {"description": "Tenant not found"}
                    }
                }
            },
            // User endpoints
            "/tenants/{tenant_id}/users": {
                "get": {
                    "tags": ["users"],
                    "summary": "List users",
                    "operationId": "listUsers",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "tenant_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "responses": {
                        "200": {"description": "List of users"}
                    }
                },
                "post": {
                    "tags": ["users"],
                    "summary": "Create a user",
                    "operationId": "createUser",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "tenant_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/CreateUserRequest"}
                            }
                        }
                    },
                    "responses": {
                        "201": {"description": "User created"}
                    }
                }
            },
            // Domain endpoints
            "/tenants/{tenant_id}/domains": {
                "get": {
                    "tags": ["domains"],
                    "summary": "List domains",
                    "operationId": "listDomains",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "tenant_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "responses": {
                        "200": {
                            "description": "List of domains",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": {"$ref": "#/components/schemas/Domain"}
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "tags": ["domains"],
                    "summary": "Create a domain",
                    "operationId": "createDomain",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "tenant_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/CreateDomainRequest"}
                            }
                        }
                    },
                    "responses": {
                        "201": {"description": "Domain created"}
                    }
                }
            },
            "/tenants/{tenant_id}/domains/{domain_id}": {
                "get": {
                    "tags": ["domains"],
                    "summary": "Get a domain",
                    "operationId": "getDomain",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "tenant_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}},
                        {"name": "domain_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "responses": {
                        "200": {"description": "Domain details with DNS records"}
                    }
                },
                "delete": {
                    "tags": ["domains"],
                    "summary": "Delete a domain",
                    "operationId": "deleteDomain",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "tenant_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}},
                        {"name": "domain_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "responses": {
                        "204": {"description": "Domain deleted"}
                    }
                }
            },
            "/tenants/{tenant_id}/domains/{domain_id}/verify": {
                "post": {
                    "tags": ["domains"],
                    "summary": "Verify domain DNS",
                    "operationId": "verifyDomain",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "tenant_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}},
                        {"name": "domain_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "responses": {
                        "200": {"description": "Verification result"}
                    }
                }
            },
            // Mailbox endpoints
            "/tenants/{tenant_id}/mailboxes": {
                "get": {
                    "tags": ["mailboxes"],
                    "summary": "List mailboxes",
                    "operationId": "listMailboxes",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "tenant_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}},
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 50}},
                        {"name": "offset", "in": "query", "schema": {"type": "integer", "default": 0}}
                    ],
                    "responses": {
                        "200": {
                            "description": "List of mailboxes",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": {"$ref": "#/components/schemas/Mailbox"}
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "tags": ["mailboxes"],
                    "summary": "Create a mailbox",
                    "operationId": "createMailbox",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "tenant_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/CreateMailboxRequest"}
                            }
                        }
                    },
                    "responses": {
                        "201": {"description": "Mailbox created"},
                        "409": {"description": "Address already exists"}
                    }
                }
            },
            // Hook endpoints
            "/tenants/{tenant_id}/hooks": {
                "get": {
                    "tags": ["hooks"],
                    "summary": "List hooks",
                    "operationId": "listHooks",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "tenant_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}},
                        {"name": "hook_type", "in": "query", "schema": {"type": "string"}},
                        {"name": "enabled_only", "in": "query", "schema": {"type": "boolean"}}
                    ],
                    "responses": {
                        "200": {"description": "List of hooks"}
                    }
                },
                "post": {
                    "tags": ["hooks"],
                    "summary": "Create a hook",
                    "operationId": "createHook",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "tenant_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/CreateHookRequest"}
                            }
                        }
                    },
                    "responses": {
                        "201": {"description": "Hook created"}
                    }
                }
            },
            "/tenants/{tenant_id}/hooks/{hook_id}/enable": {
                "post": {
                    "tags": ["hooks"],
                    "summary": "Enable a hook",
                    "operationId": "enableHook",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "tenant_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}},
                        {"name": "hook_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "responses": {
                        "200": {"description": "Hook enabled"}
                    }
                }
            },
            "/tenants/{tenant_id}/hooks/{hook_id}/disable": {
                "post": {
                    "tags": ["hooks"],
                    "summary": "Disable a hook",
                    "operationId": "disableHook",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "tenant_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}},
                        {"name": "hook_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "responses": {
                        "200": {"description": "Hook disabled"}
                    }
                }
            },
            // Send endpoint
            "/tenants/{tenant_id}/send": {
                "post": {
                    "tags": ["send"],
                    "summary": "Send an email",
                    "description": "Queue an email for delivery. The sender address must be a verified mailbox belonging to the tenant.",
                    "operationId": "sendEmail",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "tenant_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/SendEmailRequest"}
                            }
                        }
                    },
                    "responses": {
                        "202": {
                            "description": "Email queued for delivery",
                            "content": {
                                "application/json": {
                                    "schema": {"$ref": "#/components/schemas/SendEmailResponse"}
                                }
                            }
                        },
                        "400": {"description": "Invalid request"},
                        "403": {"description": "Forbidden - sender not authorized"}
                    }
                }
            },
            "/tenants/{tenant_id}/send/queue": {
                "get": {
                    "tags": ["send"],
                    "summary": "Get send queue status",
                    "operationId": "getSendQueue",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "tenant_id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "responses": {
                        "200": {
                            "description": "Queue status",
                            "content": {
                                "application/json": {
                                    "schema": {"$ref": "#/components/schemas/QueueStatusResponse"}
                                }
                            }
                        }
                    }
                }
            },
            // Messages endpoint
            "/messages": {
                "get": {
                    "tags": ["messages"],
                    "summary": "List messages",
                    "operationId": "listMessages",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "mailbox_id", "in": "query", "required": true, "schema": {"type": "string", "format": "uuid"}},
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 50}},
                        {"name": "cursor", "in": "query", "schema": {"type": "string"}}
                    ],
                    "responses": {
                        "200": {
                            "description": "List of messages",
                            "content": {
                                "application/json": {
                                    "schema": {"$ref": "#/components/schemas/MessageListResponse"}
                                }
                            }
                        }
                    }
                }
            },
            "/messages/{id}": {
                "get": {
                    "tags": ["messages"],
                    "summary": "Get a message",
                    "operationId": "getMessage",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "responses": {
                        "200": {"description": "Message details"},
                        "404": {"description": "Message not found"}
                    }
                },
                "delete": {
                    "tags": ["messages"],
                    "summary": "Delete a message",
                    "operationId": "deleteMessage",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "responses": {
                        "204": {"description": "Message deleted"}
                    }
                }
            },
            "/messages/{id}/flags": {
                "patch": {
                    "tags": ["messages"],
                    "summary": "Update message flags",
                    "operationId": "updateMessageFlags",
                    "security": [{"api_key": []}, {"bearer": []}],
                    "parameters": [
                        {"name": "id", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/UpdateFlagsRequest"}
                            }
                        }
                    },
                    "responses": {
                        "204": {"description": "Flags updated"}
                    }
                }
            }
        },
        "components": {
            "securitySchemes": {
                "api_key": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "X-API-Key"
                },
                "bearer": {
                    "type": "http",
                    "scheme": "bearer"
                }
            },
            "schemas": {
                "HealthResponse": {
                    "type": "object",
                    "properties": {
                        "status": {"type": "string", "example": "healthy"}
                    }
                },
                "DetailedHealthResponse": {
                    "type": "object",
                    "properties": {
                        "status": {"type": "string"},
                        "checks": {
                            "type": "object",
                            "properties": {
                                "database": {"$ref": "#/components/schemas/ComponentHealth"}
                            }
                        }
                    }
                },
                "ComponentHealth": {
                    "type": "object",
                    "properties": {
                        "status": {"type": "string"},
                        "latency_ms": {"type": "integer"},
                        "error": {"type": "string"}
                    }
                },
                "Tenant": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string", "format": "uuid"},
                        "name": {"type": "string"},
                        "slug": {"type": "string"},
                        "status": {"type": "string"},
                        "plan": {"type": "string"},
                        "created_at": {"type": "string", "format": "date-time"}
                    }
                },
                "CreateTenantRequest": {
                    "type": "object",
                    "required": ["name", "slug"],
                    "properties": {
                        "name": {"type": "string"},
                        "slug": {"type": "string"},
                        "plan": {"type": "string", "default": "free"}
                    }
                },
                "CreateUserRequest": {
                    "type": "object",
                    "required": ["email", "password"],
                    "properties": {
                        "email": {"type": "string", "format": "email"},
                        "password": {"type": "string", "minLength": 8},
                        "name": {"type": "string"},
                        "role": {"type": "string", "enum": ["admin", "user"], "default": "user"}
                    }
                },
                "Domain": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string", "format": "uuid"},
                        "tenant_id": {"type": "string", "format": "uuid"},
                        "name": {"type": "string"},
                        "verified": {"type": "boolean"},
                        "dkim_selector": {"type": "string"},
                        "created_at": {"type": "string", "format": "date-time"}
                    }
                },
                "CreateDomainRequest": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": {"type": "string", "example": "example.com"}
                    }
                },
                "Mailbox": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string", "format": "uuid"},
                        "tenant_id": {"type": "string", "format": "uuid"},
                        "domain_id": {"type": "string", "format": "uuid"},
                        "address": {"type": "string"},
                        "display_name": {"type": "string"},
                        "quota_bytes": {"type": "integer"},
                        "used_bytes": {"type": "integer"},
                        "usage_percent": {"type": "number"},
                        "created_at": {"type": "string", "format": "date-time"}
                    }
                },
                "CreateMailboxRequest": {
                    "type": "object",
                    "required": ["domain_id", "address"],
                    "properties": {
                        "domain_id": {"type": "string", "format": "uuid"},
                        "user_id": {"type": "string", "format": "uuid"},
                        "address": {"type": "string", "example": "user@example.com"},
                        "display_name": {"type": "string"},
                        "quota_bytes": {"type": "integer"}
                    }
                },
                "CreateHookRequest": {
                    "type": "object",
                    "required": ["name", "hook_type", "plugin_id"],
                    "properties": {
                        "name": {"type": "string"},
                        "hook_type": {"type": "string", "enum": ["pre_receive", "post_receive", "pre_send", "pre_delivery"]},
                        "plugin_id": {"type": "string"},
                        "priority": {"type": "integer", "default": 100},
                        "timeout_ms": {"type": "integer", "default": 5000},
                        "on_timeout": {"type": "string", "enum": ["continue", "reject", "tempfail"], "default": "continue"},
                        "on_error": {"type": "string", "enum": ["continue", "reject", "tempfail"], "default": "continue"},
                        "filter_config": {"type": "object"},
                        "config": {"type": "object"}
                    }
                },
                "SendEmailRequest": {
                    "type": "object",
                    "required": ["from", "to"],
                    "properties": {
                        "from": {"type": "string", "format": "email", "example": "sender@example.com"},
                        "to": {"type": "array", "items": {"type": "string", "format": "email"}, "minItems": 1},
                        "cc": {"type": "array", "items": {"type": "string", "format": "email"}},
                        "bcc": {"type": "array", "items": {"type": "string", "format": "email"}},
                        "subject": {"type": "string"},
                        "text": {"type": "string", "description": "Plain text body"},
                        "html": {"type": "string", "description": "HTML body"},
                        "reply_to": {"type": "string", "format": "email"},
                        "scheduled_at": {"type": "string", "format": "date-time"},
                        "headers": {"type": "object", "additionalProperties": {"type": "string"}},
                        "attachments": {
                            "type": "array",
                            "items": {"$ref": "#/components/schemas/Attachment"}
                        }
                    }
                },
                "Attachment": {
                    "type": "object",
                    "required": ["filename", "content_type", "content"],
                    "properties": {
                        "filename": {"type": "string"},
                        "content_type": {"type": "string"},
                        "content": {"type": "string", "format": "byte", "description": "Base64 encoded content"}
                    }
                },
                "SendEmailResponse": {
                    "type": "object",
                    "properties": {
                        "message_id": {"type": "string", "format": "uuid"},
                        "status": {"type": "string", "example": "queued"},
                        "recipients_count": {"type": "integer"},
                        "scheduled_at": {"type": "string", "format": "date-time"},
                        "queue_id": {"type": "string", "format": "uuid"}
                    }
                },
                "QueueStatusResponse": {
                    "type": "object",
                    "properties": {
                        "pending": {"type": "integer"},
                        "processing": {"type": "integer"},
                        "completed": {"type": "integer"},
                        "failed": {"type": "integer"}
                    }
                },
                "MessageListResponse": {
                    "type": "object",
                    "properties": {
                        "data": {
                            "type": "array",
                            "items": {"$ref": "#/components/schemas/MessageSummary"}
                        },
                        "cursor": {"type": "string"},
                        "has_more": {"type": "boolean"}
                    }
                },
                "MessageSummary": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string", "format": "uuid"},
                        "subject": {"type": "string"},
                        "from_address": {"type": "string"},
                        "received_at": {"type": "string", "format": "date-time"},
                        "seen": {"type": "boolean"},
                        "flagged": {"type": "boolean"},
                        "has_attachments": {"type": "boolean"}
                    }
                },
                "UpdateFlagsRequest": {
                    "type": "object",
                    "properties": {
                        "seen": {"type": "boolean"},
                        "flagged": {"type": "boolean"},
                        "answered": {"type": "boolean"},
                        "deleted": {"type": "boolean"}
                    }
                }
            }
        }
    })
}

/// Swagger UI HTML template
const SWAGGER_UI_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>MaiRust API Documentation</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" />
    <style>
        body { margin: 0; padding: 0; }
        .swagger-ui .topbar { display: none; }
    </style>
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script>
    <script>
        window.onload = function() {
            SwaggerUIBundle({
                url: "/openapi.json",
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [
                    SwaggerUIBundle.presets.apis,
                    SwaggerUIBundle.SwaggerUIStandalonePreset
                ],
                layout: "StandaloneLayout"
            });
        };
    </script>
</body>
</html>"#;
