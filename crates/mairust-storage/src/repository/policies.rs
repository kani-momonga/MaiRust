//! Policy repository

use crate::db::DatabasePool;
use crate::models::{CreatePolicyRule, PolicyRule};
use async_trait::async_trait;
use mairust_common::types::{DomainId, PolicyId, TenantId};
use mairust_common::{Error, Result};
use uuid::Uuid;

/// Policy repository trait
#[async_trait]
pub trait PolicyRepository: Send + Sync {
    async fn create(&self, input: CreatePolicyRule) -> Result<PolicyRule>;
    async fn get(&self, id: PolicyId) -> Result<Option<PolicyRule>>;
    async fn list_global(&self) -> Result<Vec<PolicyRule>>;
    async fn list_by_tenant(&self, tenant_id: TenantId) -> Result<Vec<PolicyRule>>;
    async fn list_by_domain(&self, domain_id: DomainId) -> Result<Vec<PolicyRule>>;
    async fn list_effective(&self, tenant_id: TenantId, domain_id: Option<DomainId>) -> Result<Vec<PolicyRule>>;
    async fn update(&self, id: PolicyId, input: CreatePolicyRule) -> Result<PolicyRule>;
    async fn enable(&self, id: PolicyId) -> Result<()>;
    async fn disable(&self, id: PolicyId) -> Result<()>;
    async fn delete(&self, id: PolicyId) -> Result<()>;
}

/// Database policy repository
pub struct DbPolicyRepository {
    pool: DatabasePool,
}

impl DbPolicyRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PolicyRepository for DbPolicyRepository {
    async fn create(&self, input: CreatePolicyRule) -> Result<PolicyRule> {
        let id = Uuid::now_v7();
        let now = chrono::Utc::now();

        sqlx::query(
            r#"
            INSERT INTO policy_rules (
                id, tenant_id, domain_id, name, description,
                policy_type, priority, enabled, conditions, actions,
                created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, true, $8, $9, $10, $11)
            "#,
        )
        .bind(id)
        .bind(input.tenant_id)
        .bind(input.domain_id)
        .bind(&input.name)
        .bind(&input.description)
        .bind(&input.policy_type)
        .bind(input.priority)
        .bind(&input.conditions)
        .bind(&input.actions)
        .bind(now)
        .bind(now)
        .execute(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        self.get(id)
            .await?
            .ok_or_else(|| Error::Internal("Failed to create policy rule".to_string()))
    }

    async fn get(&self, id: PolicyId) -> Result<Option<PolicyRule>> {
        sqlx::query_as::<_, PolicyRule>("SELECT * FROM policy_rules WHERE id = $1")
            .bind(id)
            .fetch_optional(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    async fn list_global(&self) -> Result<Vec<PolicyRule>> {
        sqlx::query_as::<_, PolicyRule>(
            "SELECT * FROM policy_rules WHERE tenant_id IS NULL AND domain_id IS NULL ORDER BY priority ASC",
        )
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn list_by_tenant(&self, tenant_id: TenantId) -> Result<Vec<PolicyRule>> {
        sqlx::query_as::<_, PolicyRule>(
            "SELECT * FROM policy_rules WHERE tenant_id = $1 ORDER BY priority ASC",
        )
        .bind(tenant_id)
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn list_by_domain(&self, domain_id: DomainId) -> Result<Vec<PolicyRule>> {
        sqlx::query_as::<_, PolicyRule>(
            "SELECT * FROM policy_rules WHERE domain_id = $1 ORDER BY priority ASC",
        )
        .bind(domain_id)
        .fetch_all(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))
    }

    async fn list_effective(&self, tenant_id: TenantId, domain_id: Option<DomainId>) -> Result<Vec<PolicyRule>> {
        // Get all applicable policies: global, tenant-level, and domain-level
        let query = match domain_id {
            Some(did) => {
                sqlx::query_as::<_, PolicyRule>(
                    r#"
                    SELECT * FROM policy_rules
                    WHERE enabled = true AND (
                        (tenant_id IS NULL AND domain_id IS NULL) OR
                        (tenant_id = $1 AND domain_id IS NULL) OR
                        domain_id = $2
                    )
                    ORDER BY priority ASC
                    "#,
                )
                .bind(tenant_id)
                .bind(did)
            }
            None => {
                sqlx::query_as::<_, PolicyRule>(
                    r#"
                    SELECT * FROM policy_rules
                    WHERE enabled = true AND (
                        (tenant_id IS NULL AND domain_id IS NULL) OR
                        (tenant_id = $1 AND domain_id IS NULL)
                    )
                    ORDER BY priority ASC
                    "#,
                )
                .bind(tenant_id)
            }
        };

        query
            .fetch_all(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    async fn update(&self, id: PolicyId, input: CreatePolicyRule) -> Result<PolicyRule> {
        let now = chrono::Utc::now();

        sqlx::query(
            r#"
            UPDATE policy_rules SET
                tenant_id = $2,
                domain_id = $3,
                name = $4,
                description = $5,
                policy_type = $6,
                priority = $7,
                conditions = $8,
                actions = $9,
                updated_at = $10
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(input.tenant_id)
        .bind(input.domain_id)
        .bind(&input.name)
        .bind(&input.description)
        .bind(&input.policy_type)
        .bind(input.priority)
        .bind(&input.conditions)
        .bind(&input.actions)
        .bind(now)
        .execute(self.pool.pool())
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        self.get(id)
            .await?
            .ok_or_else(|| Error::Internal("Policy rule not found".to_string()))
    }

    async fn enable(&self, id: PolicyId) -> Result<()> {
        let now = chrono::Utc::now();
        sqlx::query("UPDATE policy_rules SET enabled = true, updated_at = $2 WHERE id = $1")
            .bind(id)
            .bind(now)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn disable(&self, id: PolicyId) -> Result<()> {
        let now = chrono::Utc::now();
        sqlx::query("UPDATE policy_rules SET enabled = false, updated_at = $2 WHERE id = $1")
            .bind(id)
            .bind(now)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: PolicyId) -> Result<()> {
        sqlx::query("DELETE FROM policy_rules WHERE id = $1")
            .bind(id)
            .execute(self.pool.pool())
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }
}
