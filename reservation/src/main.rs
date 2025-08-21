use reservation::ReservationService;
use tonic::{transport::Server};
use std::env;
use dotenv::dotenv;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 加载环境变量
    dotenv().ok();

    // 获取数据库连接字符串
    let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:password@localhost/reservation".to_string()
    });

    // 初始化预订服务
    let reservation_service = ReservationService::new(&db_url).await?;

    // 设置服务器地址
    let addr = "[::1]:50051".parse()?;

    println!("Reservation service starting on {}", addr);

    // 启动服务器
    Server::builder()
        .add_service(reservation::reservation_service_server::ReservationServiceServer::new(reservation_service))
        .serve(addr)
        .await?;

    Ok(())
}