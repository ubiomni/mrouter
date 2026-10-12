//! Permission utilities for role-based access control
//!
//! Roles: super_admin > org_admin > user
//! - super_admin: full access across all organizations (org_id=0)
//! - org_admin: full access to all resources within their organization
//! - user: access only to their own resources

use super::auth::Claims;

/// Role hierarchy level (higher = more permissions)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoleLevel {
    User = 0,
    OrgAdmin = 1,
    SuperAdmin = 2,
}

impl RoleLevel {
    pub fn from_str(s: &str) -> Self {
        match s {
            "super_admin" | "superadmin" => RoleLevel::SuperAdmin,
            "org_admin" | "orgadmin" => RoleLevel::OrgAdmin,
            _ => RoleLevel::User,
        }
    }
}

/// Check if claims have at least the required role level
pub fn has_role(claims: &Claims, minimum: RoleLevel) -> bool {
    RoleLevel::from_str(&claims.role) >= minimum
}

/// Check if claims indicate super_admin
pub fn is_super_admin(claims: &Claims) -> bool {
    has_role(claims, RoleLevel::SuperAdmin)
}

/// Check if claims indicate org_admin or above
pub fn is_org_admin(claims: &Claims) -> bool {
    has_role(claims, RoleLevel::OrgAdmin)
}

/// Resource scope for filtering queries based on role
#[derive(Debug, Clone)]
pub enum ResourceScope {
    /// super_admin: see everything across all orgs
    Global,
    /// org_admin: see everything within their org (no filter within org)
    All,
    /// user: filter by user_id only
    User(i64),
}

/// Determine the resource scope based on claims
pub fn resource_scope(claims: &Claims) -> ResourceScope {
    let level = RoleLevel::from_str(&claims.role);
    match level {
        RoleLevel::SuperAdmin => ResourceScope::Global,
        RoleLevel::OrgAdmin => ResourceScope::All,
        RoleLevel::User => ResourceScope::User(claims.user_id),
    }
}
