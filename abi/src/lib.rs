use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 预订状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReservationStatus {
    /// 未知状态
    Unknown,
    /// 待确认
    Pending,
    /// 已确认
    Confirmed,
    /// 已阻塞
    Blocked,
}

/// 预订更新类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReservationUpdateType {
    /// 未知更新类型
    Unknown,
    /// 创建预订
    Create,
    /// 更新预订
    Update,
    /// 删除预订
    Delete,
}

/// 预订结构体
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reservation {
    /// 预订ID
    pub id: Uuid,
    /// 用户ID
    pub user_id: String,
    /// 预订状态
    pub status: ReservationStatus,
    /// 资源ID
    pub resource_id: String,
    /// 开始时间
    pub start: DateTime<Utc>,
    /// 结束时间
    pub end: DateTime<Utc>,
    /// 备注信息
    pub note: Option<String>,
}

/// 预订请求
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReserveRequest {
    /// 预订信息
    pub reservation: Reservation,
}

/// 预订响应
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReserveResponse {
    /// 预订信息
    pub reservation: Reservation,
}

/// 更新请求
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateRequest {
    /// 预订ID
    pub id: Uuid,
    /// 备注信息
    pub note: Option<String>,
}

/// 更新响应
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateResponse {
    /// 预订信息
    pub reservation: Reservation,
}

/// 确认请求
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmRequest {
    /// 预订ID
    pub id: Uuid,
}

/// 确认响应
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmResponse {
    /// 预订信息
    pub reservation: Reservation,
}

/// 取消请求
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelRequest {
    /// 预订ID
    pub id: Uuid,
}

/// 取消响应
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelResponse {
    /// 预订信息
    pub reservation: Reservation,
}

/// 获取请求
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetRequest {
    /// 预订ID
    pub id: Uuid,
}

/// 获取响应
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetResponse {
    /// 预订信息
    pub reservation: Reservation,
}

/// 查询请求
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryRequest {
    /// 资源ID
    pub resource_id: Option<String>,
    /// 用户ID
    pub user_id: Option<String>,
    /// 预订状态
    pub status: Option<ReservationStatus>,
    /// 开始时间
    pub start: Option<DateTime<Utc>>,
    /// 结束时间
    pub end: Option<DateTime<Utc>>,
}

/// 变更通知请求
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListenRequest {
    // 留空，用于建立监听连接
}

/// 变更通知响应
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListenResponse {
    /// 操作类型
    pub op: ReservationUpdateType,
    /// 预订信息
    pub reservation: Reservation,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_reservation_creation() {
        let now = Utc::now();
        let reservation = Reservation {
            id: Uuid::new_v4(),
            user_id: "user123".to_string(),
            status: ReservationStatus::Pending,
            resource_id: "resource456".to_string(),
            start: now,
            end: now + Duration::hours(1),
            note: Some("Test reservation".to_string()),
        };

        assert_eq!(reservation.status, ReservationStatus::Pending);
        assert_eq!(reservation.user_id, "user123");
    }
}
