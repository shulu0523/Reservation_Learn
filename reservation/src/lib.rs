use chrono::{Utc};
use thiserror::Error;
use tokio_postgres::{Client, NoTls};
use futures::{Stream, stream::StreamExt};
use std::sync::Arc;
use tonic::{transport::Server, Request, Response, Status};

// 导入生成的gRPC代码
mod reservation {
    tonic::include_proto!("reservation");
}

use reservation::reservation_service_server::{ReservationService as ReservationServiceTrait, ReservationServiceServer};
use reservation::{ConfirmRequest, ConfirmResponse, CancelRequest, CancelResponse, GetRequest, GetResponse, QueryRequest, ReserveRequest, ReserveResponse, UpdateRequest, UpdateResponse, ListenRequest, ListenResponse, Reservation, ReservationStatus, ReservationUpdateType};

/// 预订服务错误类型
#[derive(Error, Debug)]
pub enum ReservationError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] tokio_postgres::Error),
    
    #[error("Reservation conflict: {0}")]
    Conflict(String),
    
    #[error("Reservation not found")]
    NotFound,
    
    #[error("Invalid reservation status transition")]
    InvalidStatusTransition,
    
    #[error("Invalid time range")]
    InvalidTimeRange,
}

/// 预订服务结果类型
pub type Result<T> = std::result::Result<T, ReservationError>;

/// 预订服务
#[derive(Debug)]
pub struct ReservationService {
    client: Arc<Client>,
}

#[tonic::async_trait]
impl ReservationServiceTrait for ReservationService {
    // 实现gRPC服务方法

    /// 创建预订
    async fn reserve(&self, request: Request<ReserveRequest>) -> Result<Response<ReserveResponse>, Status> {
        let inner = request.into_inner();
        let reservation = Reservation {
            id: uuid::Uuid::new_v4().to_string(),
            resource_id: inner.resource_id,
            start_time: Some(inner.start_time.unwrap_or_default().into()),
            end_time: Some(inner.end_time.unwrap_or_default().into()),
            user_id: inner.user_id,
            status: ReservationStatus::StatusPending as i32,
            metadata: inner.metadata,
            created_at: Some(Utc::now().into()),
            updated_at: Some(Utc::now().into()),
        };

        // 验证时间范围
        if reservation.start_time.as_ref().unwrap().seconds() >= reservation.end_time.as_ref().unwrap().seconds() {
            return Err(Status::invalid_argument("Invalid time range: start time must be before end time"));
        }

        // 检查冲突
        let conflict = self.check_conflict(&reservation).await;
        if conflict {
            return Err(Status::already_exists(format!(
                "Reservation conflict for resource {} from {:?} to {:?}",
                reservation.resource_id,
                reservation.start_time,
                reservation.end_time
            )));
        }

        // 插入预订记录
        match self.insert_reservation(&reservation).await {
            Ok(_) => Ok(Response::new(ReserveResponse {
                reservation_id: reservation.id.clone(),
                status: reservation.status,
                message: "Reservation created successfully".to_string(),
            })),
            Err(e) => Err(Status::internal(format!("Failed to create reservation: {:?}", e))),
        }
    }

    /// 确认预订
    async fn confirm(&self, request: Request<ConfirmRequest>) -> Result<Response<ConfirmResponse>, Status> {
        let inner = request.into_inner();

        // 更新预订状态
        match self.update_reservation_status(&inner.reservation_id, ReservationStatus::StatusConfirmed).await {
            Ok(updated) if updated > 0 => Ok(Response::new(ConfirmResponse {
                success: true,
                message: "Reservation confirmed successfully".to_string(),
            })),
            Ok(_) => Err(Status::not_found(format!("Reservation not found: {}", inner.reservation_id))),
            Err(e) => Err(Status::internal(format!("Failed to confirm reservation: {:?}", e))),
        }
    }

    /// 取消预订
    async fn cancel(&self, request: Request<CancelRequest>) -> Result<Response<CancelResponse>, Status> {
        let inner = request.into_inner();

        // 更新预订状态
        match self.update_reservation_status(&inner.reservation_id, ReservationStatus::StatusCancelled).await {
            Ok(updated) if updated > 0 => Ok(Response::new(CancelResponse {
                success: true,
                message: format!("Reservation cancelled: {}", inner.reason.unwrap_or_default()),
            })),
            Ok(_) => Err(Status::not_found(format!("Reservation not found: {}", inner.reservation_id))),
            Err(e) => Err(Status::internal(format!("Failed to cancel reservation: {:?}", e))),
        }
    }

    /// 获取预订
    async fn get(&self, request: Request<GetRequest>) -> Result<Response<GetResponse>, Status> {
        let inner = request.into_inner();

        match self.get_reservation(&inner.reservation_id).await {
            Ok(reservation) => Ok(Response::new(GetResponse {
                reservation: Some(reservation),
            })),
            Err(ReservationError::NotFound) => Err(Status::not_found(format!("Reservation not found: {}", inner.reservation_id))),
            Err(e) => Err(Status::internal(format!("Failed to get reservation: {:?}", e))),
        }
    }

    /// 更新预订
    async fn update(&self, request: Request<UpdateRequest>) -> Result<Response<UpdateResponse>, Status> {
        let inner = request.into_inner();

        // 获取现有预订
        let existing = match self.get_reservation(&inner.reservation_id).await {
            Ok(reservation) => reservation,
            Err(ReservationError::NotFound) => return Err(Status::not_found(format!("Reservation not found: {}", inner.reservation_id))),
            Err(e) => return Err(Status::internal(format!("Failed to get reservation: {:?}", e))),
        };

        // 构建更新后的预订
        let updated = Reservation {
            id: existing.id,
            resource_id: inner.resource_id.unwrap_or(existing.resource_id),
            start_time: inner.start_time.or(existing.start_time),
            end_time: inner.end_time.or(existing.end_time),
            user_id: existing.user_id,
            status: existing.status,
            metadata: inner.metadata.unwrap_or(existing.metadata),
            created_at: existing.created_at,
            updated_at: Some(Utc::now().into()),
        };

        // 验证时间范围
        if updated.start_time.as_ref().unwrap().seconds() >= updated.end_time.as_ref().unwrap().seconds() {
            return Err(Status::invalid_argument("Invalid time range: start time must be before end time"));
        }

        // 检查冲突
        let conflict = self.check_conflict_excluding(&updated, &updated.id).await;
        if conflict {
            return Err(Status::already_exists(format!(
                "Reservation conflict for resource {} from {:?} to {:?}",
                updated.resource_id,
                updated.start_time,
                updated.end_time
            )));
        }

        // 更新预订记录
        match self.update_reservation(&updated).await {
            Ok(_) => Ok(Response::new(UpdateResponse {
                success: true,
                message: "Reservation updated successfully".to_string(),
                reservation: Some(updated),
            })),
            Err(e) => Err(Status::internal(format!("Failed to update reservation: {:?}", e))),
        }
    }

    /// 查询预订列表
    async fn query(&self, request: Request<QueryRequest>) -> Result<Response<QueryResponse>, Status> {
        let inner = request.into_inner();

        match self.query_reservations(&inner).await {
            Ok(reservations) => Ok(Response::new(QueryResponse {
                reservations,
                next_page_token: "".to_string(), // 简化实现，不支持分页
                total_count: reservations.len() as i32,
            })),
            Err(e) => Err(Status::internal(format!("Failed to query reservations: {:?}", e))),
        }
    }

    /// 监听预订变更
    type ListenStream = impl Stream<Item = Result<ListenResponse, Status>>;

    async fn listen(&self, request: Request<ListenRequest>) -> Result<Response<Self::ListenStream>, Status> {
        let inner = request.into_inner();

        // 订阅通知
        if let Err(e) = self.client.execute("LISTEN reservation_update", &[]).await {
            return Err(Status::internal(format!("Failed to subscribe to notifications: {:?}", e)));
        }

        // 创建变更流
        let stream = (&*self.client).notifications();
        let client = self.client.clone();
        let resource_id = inner.resource_id;
        let user_id = inner.user_id;

        // 转换通知流
        let mapped_stream = stream.map(move |notification| {
            let client = client.clone();
            let resource_id = resource_id.clone();
            let user_id = user_id.clone();

            async move {
                // 解析通知内容
                let payload = notification.payload().map(|p| p.to_string()).unwrap_or_default();
                let parts: Vec<&str> = payload.split(',').collect();

                if parts.len() < 3 {
                    return Err(Status::invalid_argument("Invalid notification format"));
                }

                let reservation_id = parts[0].to_string();
                let update_type = match parts[1] {
                    "created" => ReservationUpdateType::UpdateTypeCreated,
                    "confirmed" => ReservationUpdateType::UpdateTypeConfirmed,
                    "cancelled" => ReservationUpdateType::UpdateTypeCancelled,
                    "modified" => ReservationUpdateType::UpdateTypeModified,
                    _ => ReservationUpdateType::UpdateTypeUnspecified,
                };

                // 获取更新后的预订
                match client.clone().get_reservation(&reservation_id).await {
                    Ok(reservation) => {
                        // 筛选资源ID和用户ID
                        if (!resource_id.is_empty() && reservation.resource_id != resource_id) ||
                           (!user_id.is_empty() && reservation.user_id != user_id) {
                            return Err(Status::failed_precondition("Filtered out"));
                        }

                        Ok(ListenResponse {
                            reservation: Some(reservation),
                            update_type: update_type as i32,
                            timestamp: Some(Utc::now().into()),
                        })
                    },
                    Err(e) => Err(Status::internal(format!("Failed to get updated reservation: {:?}", e))),
                }
            }
        });

        Ok(Response::new(mapped_stream))
    }
}

impl ReservationService {
    /// 创建新的预订服务实例
    pub async fn new(connection_string: &str) -> Result<Self> {
        let (client, connection) = tokio_postgres::connect(connection_string, NoTls).await?;
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Connection error: {}", e);
            }
        });
        Ok(Self { client: Arc::new(client) })
    }
    
    /// 创建预订
    pub async fn reserve(&self, request: ReserveRequest) -> Result<ReserveResponse> {
        let reservation = request.reservation;
        
        // 验证时间范围
        if reservation.start >= reservation.end {
            return Err(ReservationError::InvalidTimeRange);
        }
        
        // 检查状态是否合法
        if !matches!(reservation.status, ReservationStatus::Pending) {
            return Err(ReservationError::InvalidStatusTransition);
        }
        
        // 插入预订记录
        let status_str = format!("{:?}", reservation.status).to_lowercase();
        let result = self.client.execute(
            "INSERT INTO rsvp.reservations (id, user_id, status, resource_id, timespan, note)\n             VALUES ($1, $2, $3::rsvp.reservation_status, $4, TSTZRANGE($5, $6), $7)",
            &[
                &reservation.id,
                &reservation.user_id,
                &status_str,
                &reservation.resource_id,
                &reservation.start,
                &reservation.end,
                &reservation.note,
            ],
        ).await;
        
        match result {
            Ok(_) => Ok(ReserveResponse { reservation }),
            Err(e) => {
                // 检查是否是冲突错误
                if e.to_string().contains("conflict") {
                    Err(ReservationError::Conflict(format!(
                        "Reservation conflict for resource {} from {} to {}",
                        reservation.resource_id, reservation.start, reservation.end
// 原代码中错误使用了 `{}` 占位符，应改为 `{}` 带索引或正确的占位符数量
// 以下为修正后的占位符对应的参数使用
// 修正后的错误消息创建逻辑中应将错误消息的占位符改为 `{}`，但此处仅修改选择部分代码

// 注意：调用处的错误消息字符串 `"Reservation conflict for resource {{}} from {{}} to {{}}"` 
// 应修改为 `"Reservation conflict for resource {} from {} to {}"`，但此部分不在选择范围内
                    )))
                } else {
                    Err(ReservationError::DatabaseError(e))
                }
            }
        }
    }
    
    /// 确认预订
    pub async fn confirm(&self, request: ConfirmRequest) -> Result<ConfirmResponse> {
        let id = request.id;
        
        // 更新预订状态
        let result = self.client.execute(
            "UPDATE rsvp.reservations SET status = 'confirmed' WHERE id = $1 AND status = 'pending'",
            &[&id],
        ).await?;
        
        if result == 0 {
            return Err(ReservationError::NotFound);
        }
        
        // 获取更新后的预订
        self.get(GetRequest { id }).await.map(|response| {
            ConfirmResponse { reservation: response.reservation }
        })
    }
    
    /// 取消预订
    pub async fn cancel(&self, request: CancelRequest) -> Result<CancelResponse> {
        let id = request.id;
        
        // 更新预订状态为阻塞
        let result = self.client.execute(
            "UPDATE rsvp.reservations SET status = 'blocked' WHERE id = $1",
            &[&id],
        ).await?;
        
        if result == 0 {
            return Err(ReservationError::NotFound);
        }
        
        // 获取更新后的预订
        self.get(GetRequest { id }).await.map(|response| {
            CancelResponse { reservation: response.reservation }
        })
    }
    
    /// 获取预订
    pub async fn get(&self, request: GetRequest) -> Result<GetResponse> {
        let id = request.id;
        
        // 查询预订
        let row = self.client.query_one(
            "SELECT id, user_id, status, resource_id, lower(timespan) as start, upper(timespan) as end, note\n             FROM rsvp.reservations WHERE id = $1",
            &[&id],
        ).await;
        
        match row {
            Ok(row) => {
                let status_str: String = row.get("status");
                let status = match status_str.as_str() {
                    "pending" => ReservationStatus::Pending,
                    "confirmed" => ReservationStatus::Confirmed,
                    "blocked" => ReservationStatus::Blocked,
                    _ => ReservationStatus::Unknown,
                };
                
                let reservation = Reservation {
                    id: row.get("id"),
                    user_id: row.get("user_id"),
                    status,
                    resource_id: row.get("resource_id"),
                    start: row.get("start"),
                    end: row.get("end"),
                    note: row.get("note"),
                };
                
                Ok(GetResponse { reservation })
            }
            Err(e) => {
                if e.to_string().contains("no rows returned") {
                    Err(ReservationError::NotFound)
                } else {
                    Err(ReservationError::DatabaseError(e))
                }
            }
        }
    }
    
    /// 更新预订备注
    pub async fn update(&self, request: UpdateRequest) -> Result<UpdateResponse> {
        let id = request.id;
        let note = request.note;
        
        // 更新预订备注
        let result = self.client.execute(
            "UPDATE rsvp.reservations SET note = $1 WHERE id = $2",
            &[&note, &id],
        ).await?;
        
        if result == 0 {
            return Err(ReservationError::NotFound);
        }
        
        // 获取更新后的预订
        self.get(GetRequest { id }).await.map(|response| {
            UpdateResponse { reservation: response.reservation }
        })
    }
    
    /// 查询预订列表
    pub async fn query(&self, request: QueryRequest) -> Result<Vec<Reservation>> {
        // 构建查询语句
        let mut query = "SELECT id, user_id, status, resource_id, lower(timespan) as start, upper(timespan) as end, note FROM rsvp.reservations WHERE 1=1".to_string();
        let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync>> = Vec::new();
        
        // 添加查询条件
        if let Some(resource_id) = &request.resource_id {
            query.push_str(" AND resource_id = $");
            query.push_str(&(params.len() + 1).to_string());
            params.push(Box::new(resource_id.clone()));
        }
        
        if let Some(user_id) = &request.user_id {
            query.push_str(" AND user_id = $");
            query.push_str(&(params.len() + 1).to_string());
            params.push(Box::new(user_id.clone()));
        }
        
        if let Some(status) = &request.status {
            query.push_str(" AND status = $");
            query.push_str(&(params.len() + 1).to_string());
            query.push_str("::rsvp.reservation_status");
            let status_str = format!("{:?}", status).to_lowercase();
            params.push(Box::new(status_str));
        }
        
        if let Some(start) = &request.start {
            query.push_str(" AND timespan && TSTZRANGE($");
            query.push_str(&(params.len() + 1).to_string());
            query.push_str(", $");
            query.push_str(&(params.len() + 2).to_string());
            query.push_str(")");
            params.push(Box::new(start.clone()));
            let end = request.end.unwrap_or_else(|| Utc::now() + chrono::Duration::days(365));
            params.push(Box::new(end));
        }
        
        // 执行查询
        let rows = self.client.query(&query, &params.iter().map(|p| &**p as &(dyn tokio_postgres::types::ToSql + Sync)).collect::<Vec<_>>()).await?;
        
        // 处理结果
        let mut reservations = Vec::new();
        for row in rows {
            let status_str: String = row.get("status");
            let status = match status_str.as_str() {
                "pending" => ReservationStatus::Pending,
                "confirmed" => ReservationStatus::Confirmed,
                "blocked" => ReservationStatus::Blocked,
                _ => ReservationStatus::Unknown,
            };
            
            reservations.push(Reservation {
                id: row.get("id"),
                user_id: row.get("user_id"),
                status,
                resource_id: row.get("resource_id"),
                start: row.get("start"),
                end: row.get("end"),
                note: row.get("note"),
            });
        }
        
        Ok(reservations)
    }
    
    // 数据库交互辅助方法
    async fn check_conflict(&self, reservation: &Reservation) -> bool {
        let start_time = reservation.start_time.as_ref().unwrap();
        let end_time = reservation.end_time.as_ref().unwrap();

        let query = "SELECT COUNT(*) FROM reservations WHERE resource_id = $1 AND status != $2 AND (start_time < $4 AND end_time > $3)";

        match self.client.query_one(query, &[
            &reservation.resource_id,
            &ReservationStatus::StatusCancelled as i32,
            &start_time,
            &end_time,
        ]).await {
            Ok(row) => row.get::<_, i64>(0) > 0,
            Err(_) => false,
        }
    }

    async fn check_conflict_excluding(&self, reservation: &Reservation, exclude_id: &str) -> bool {
        let start_time = reservation.start_time.as_ref().unwrap();
        let end_time = reservation.end_time.as_ref().unwrap();

        let query = "SELECT COUNT(*) FROM reservations WHERE id != $1 AND resource_id = $2 AND status != $3 AND (start_time < $5 AND end_time > $4)";

        match self.client.query_one(query, &[
            &exclude_id,
            &reservation.resource_id,
            &ReservationStatus::StatusCancelled as i32,
            &start_time,
            &end_time,
        ]).await {
            Ok(row) => row.get::<_, i64>(0) > 0,
            Err(_) => false,
        }
    }

    async fn insert_reservation(&self, reservation: &Reservation) -> Result<()>
    {
        let query = "INSERT INTO reservations (id, resource_id, start_time, end_time, user_id, status, metadata, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)";

        self.client.execute(query, &[
            &reservation.id,
            &reservation.resource_id,
            &reservation.start_time,
            &reservation.end_time,
            &reservation.user_id,
            &reservation.status,
            &reservation.metadata,
            &reservation.created_at,
            &reservation.updated_at,
        ]).await?;

        // 发送通知
        let payload = format!("{},created,{}", reservation.id, reservation.resource_id);
        self.client.execute("NOTIFY reservation_update, $1", &[&payload]).await?;

        Ok(())
    }

    async fn update_reservation_status(&self, reservation_id: &str, status: ReservationStatus) -> Result<u64>
    {
        let query = "UPDATE reservations SET status = $1, updated_at = $2 WHERE id = $3";

        let result = self.client.execute(query, &[
            &status as i32,
            &Utc::now().into(),
            &reservation_id,
        ]).await?;

        // 发送通知
        if result > 0 {
            let status_str = match status {
                ReservationStatus::StatusConfirmed => "confirmed",
                ReservationStatus::StatusCancelled => "cancelled",
                _ => "modified",
            };
            let payload = format!("{},{},{}", reservation_id, status_str, Utc::now());
            self.client.execute("NOTIFY reservation_update, $1", &[&payload]).await?;
        }

        Ok(result)
    }

    async fn get_reservation(&self, reservation_id: &str) -> Result<Reservation>
    {
        let query = "SELECT id, resource_id, start_time, end_time, user_id, status, metadata, created_at, updated_at FROM reservations WHERE id = $1";

        let row = self.client.query_one(query, &[&reservation_id]).await
            .map_err(|e| if e.to_string().contains("no rows returned") { ReservationError::NotFound } else { ReservationError::DatabaseError(e) })?;

        Ok(Reservation {
            id: row.get("id"),
            resource_id: row.get("resource_id"),
            start_time: row.get("start_time"),
            end_time: row.get("end_time"),
            user_id: row.get("user_id"),
            status: row.get("status"),
            metadata: row.get("metadata"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    async fn update_reservation(&self, reservation: &Reservation) -> Result<()>
    {
        let query = "UPDATE reservations SET resource_id = $1, start_time = $2, end_time = $3, metadata = $4, updated_at = $5 WHERE id = $6";

        self.client.execute(query, &[
            &reservation.resource_id,
            &reservation.start_time,
            &reservation.end_time,
            &reservation.metadata,
            &reservation.updated_at,
            &reservation.id,
        ]).await?;

        // 发送通知
        let payload = format!("{},modified,{}", reservation.id, reservation.resource_id);
        self.client.execute("NOTIFY reservation_update, $1", &[&payload]).await?;

        Ok(())
    }

    async fn query_reservations(&self, request: &QueryRequest) -> Result<Vec<Reservation>>
    {
        let mut query = "SELECT id, resource_id, start_time, end_time, user_id, status, metadata, created_at, updated_at FROM reservations WHERE 1=1".to_string();
        let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync>> = Vec::new();

        if !request.resource_id.is_empty() {
            query.push_str(" AND resource_id = $");
            query.push_str(&(params.len() + 1).to_string());
            params.push(Box::new(request.resource_id.clone()));
        }

        if !request.user_id.is_empty() {
            query.push_str(" AND user_id = $");
            query.push_str(&(params.len() + 1).to_string());
            params.push(Box::new(request.user_id.clone()));
        }

        if request.status != ReservationStatus::StatusUnspecified as i32 {
            query.push_str(" AND status = $");
            query.push_str(&(params.len() + 1).to_string());
            params.push(Box::new(request.status));
        }

        if let Some(start_time) = &request.start_time {
            query.push_str(" AND end_time > $");
            query.push_str(&(params.len() + 1).to_string());
            params.push(Box::new(start_time));
        }

        if let Some(end_time) = &request.end_time {
            query.push_str(" AND start_time < $");
            query.push_str(&(params.len() + 1).to_string());
            params.push(Box::new(end_time));
        }

        // 添加分页
        if request.page_size > 0 {
            query.push_str(" ORDER BY created_at DESC LIMIT $");
            query.push_str(&(params.len() + 1).to_string());
            params.push(Box::new(request.page_size as i64));

            if !request.page_token.is_empty() {
                // 简化实现，不真正支持分页令牌
            }
        }

        let rows = self.client.query(&query, &params.iter().map(|p| &**p as &(dyn tokio_postgres::types::ToSql + Sync)).collect::<Vec<_>>()).await?;

        let mut reservations = Vec::new();
        for row in rows {
            reservations.push(Reservation {
                id: row.get("id"),
                resource_id: row.get("resource_id"),
                start_time: row.get("start_time"),
                end_time: row.get("end_time"),
                user_id: row.get("user_id"),
                status: row.get("status"),
                metadata: row.get("metadata"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            });
        }

        Ok(reservations)
    }
            async move {
                // 当收到通知时，查询最近的变更
                let rows = client.query(
                    "SELECT c.id, c.reservation_id, c.op, r.id as r_id, r.user_id, r.status, r.resource_id, lower(r.timespan) as start, upper(r.timespan) as end, r.note\n                     FROM rsvp.reservation_changes c\n                     JOIN rsvp.reservations r ON c.reservation_id = r.id\n                     ORDER BY c.id DESC\n                     LIMIT 1",
                    &[],
                ).await?;
                
                if let Some(row) = rows.into_iter().next() {
                    let op_str: String = row.get("op");
                    let op = match op_str.as_str() {
                        "create" => ReservationUpdateType::Create,
                        "update" => ReservationUpdateType::Update,
                        "delete" => ReservationUpdateType::Delete,
                        _ => ReservationUpdateType::Unknown,
                    };
                    
                    let status_str: String = row.get("status");
                    let status = match status_str.as_str() {
                        "pending" => ReservationStatus::Pending,
                        "confirmed" => ReservationStatus::Confirmed,
                        "blocked" => ReservationStatus::Blocked,
                        _ => ReservationStatus::Unknown,
                    };
                    
                    let reservation = Reservation {
                        id: row.get("r_id"),
                        user_id: row.get("user_id"),
                        status,
                        resource_id: row.get("resource_id"),
                        start: row.get("start"),
                        end: row.get("end"),
                        note: row.get("note"),
                    };
                    
                    // 删除已处理的变更记录
                    client.execute("DELETE FROM rsvp.reservation_changes WHERE id = $1", &[&row.get("id")]).await?;
                    
                    Ok(ListenResponse { op, reservation })
                } else {
                    Err(ReservationError::NotFound)
                }
            }
        }).buffer_unordered(10);
        
        Ok(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    
    // 测试数据库连接和基本操作
    #[tokio::test]
    async fn test_reservation_service() {
        // 注意：实际测试应该使用测试数据库
        let connection_string = "postgresql://postgres:password@localhost/reservation_test";
        let service = ReservationService::new(connection_string).await.expect("Failed to create service");
        
        // 创建一个测试预订
        let now = Utc::now();
        let reservation = Reservation {
            id: Uuid::new_v4(),
            user_id: "test_user".to_string(),
            status: ReservationStatus::Pending,
            resource_id: "test_resource".to_string(),
            start: now,
            end: now + Duration::hours(1),
            note: Some("Test reservation".to_string()),
        };
        
        // 测试创建预订
        let reserve_response = service.reserve(ReserveRequest { reservation: reservation.clone() }).await;
        assert!(reserve_response.is_ok());
        
        // 测试获取预订
        let get_response = service.get(GetRequest { id: reservation.id }).await;
        assert!(get_response.is_ok());
        let retrieved_reservation = get_response.unwrap().reservation;
        assert_eq!(retrieved_reservation.id, reservation.id);
        assert_eq!(retrieved_reservation.user_id, reservation.user_id);
        
        // 测试确认预订
        let confirm_response = service.confirm(ConfirmRequest { id: reservation.id }).await;
        assert!(confirm_response.is_ok());
        assert_eq!(confirm_response.unwrap().reservation.status, ReservationStatus::Confirmed);
        
        // 测试更新预订
        let update_response = service.update(UpdateRequest { 
            id: reservation.id, 
            note: Some("Updated note".to_string()) 
        }).await;
        assert!(update_response.is_ok());
        assert_eq!(update_response.unwrap().reservation.note, Some("Updated note".to_string()));
        
        // 测试取消预订
        let cancel_response = service.cancel(CancelRequest { id: reservation.id }).await;
        assert!(cancel_response.is_ok());
        assert_eq!(cancel_response.unwrap().reservation.status, ReservationStatus::Blocked);
        
        // 测试查询预订
        let query_response = service.query(QueryRequest {
            resource_id: Some("test_resource".to_string()),
            user_id: None,
            status: None,
            start: None,
            end: None,
        }).await;
        assert!(query_response.is_ok());
        assert!(!query_response.unwrap().is_empty());
    }
}
