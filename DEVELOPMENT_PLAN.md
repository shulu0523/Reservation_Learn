# 核心预订服务开发计划

基于RFCS/0001-core-reservation.md文档，制定以下开发计划：

## 1. 项目概述
- 开发一个核心预订服务，解决资源在特定时间段内的预订问题
- 使用PostgreSQL的EXCLUDE约束确保同一资源在同一时间只能有一个预订
- 采用gRPC作为服务接口

## 2. 技术栈
- 编程语言: Rust
- 数据库: PostgreSQL
- API框架: gRPC 使用 tonic 库
- 项目管理: Cargo Workspace

## 3. 项目结构
```
Reservation_Learn/
├── abi/             # 协议定义和数据结构
├── reservation/     # 核心预订逻辑
├── service/         # gRPC服务实现
└── rfcs/            # 需求文档
```

## 4. 开发阶段

### 阶段1: 环境设置与基础架构 (1周)
1. 初始化Cargo workspace
2. 设置PostgreSQL数据库
3. 配置开发环境和工具链
4. 创建基础项目结构

### 阶段2: 数据模型与存储层 (1周)
1. 在abi中定义核心数据结构
2. 实现数据库模式(schema)迁移
3. 开发数据库交互层
4. 实现基于PostgreSQL EXCLUDE约束的冲突检测

### 阶段3: 核心业务逻辑 (1.5周)
1. 实现预订创建、确认、更新、取消等核心功能
2. 开发预订查询功能
3. 实现预订冲突检测逻辑
4. 编写单元测试

### 阶段4: gRPC服务实现 (1周)
1. 定义gRPC服务接口
2. 实现服务端逻辑
3. 开发客户端示例
4. 集成业务逻辑与gRPC接口

### 阶段5: 变更通知机制 (0.5周)
1. 实现数据库触发器
2. 开发变更监听服务
3. 实现基于通知的实时更新

### 阶段6: 测试与优化 (1周)
1. 编写集成测试
2. 性能测试与优化
3. 安全测试
4. 文档完善

## 5. 关键技术实现点

### 数据库层
```sql
-- 实现EXCLUDE约束防止重叠预订
CREATE TABLE rsvp.reservations (
    id uuid NOT NULL DEFAULT uuid_generate_v4(),
    user_id VARCHAR(64) NOT NULL,
    status rsvp.reservation_status NOT NULL DEFAULT 'pending',
    resource_id VARCHAR(64) NOT NULL,
    timespan TSTZRANGE NOT NULL,
    note TEXT,
    CONSTRAINT reservations_pkey PRIMARY KEY (id),
    CONSTRAINT reservations_conflict EXCLUDE USING gist (resource_id WITH =, timespan WITH &&)
);
```

### gRPC服务定义
```proto
service ReservationService {
    rpc reserve(ReserveRequest) returns (ReserveResponse);
    rpc confirm(ConfirmRequest) returns (ConfirmResponse);
    rpc update(UpdateRequest) returns (UpdateResponse);
    rpc cancel(CancelRequest) returns (CancelResponse);
    rpc get(GetRequest) returns (GetResponse);
    rpc query(QueryRequest) returns (stream Reservation);
    rpc listen(ListenRequest) returns (stream Reservation);
}
```

## 6. 里程碑与交付物
- 第2周: 完成数据模型和存储层实现
- 第3.5周: 完成核心业务逻辑实现
- 第4.5周: 完成gRPC服务实现
- 第5周: 完成变更通知机制
- 第6周: 完成测试、优化和文档

## 7. 风险与应对
- 数据库性能问题: 提前设计索引，进行性能测试
- 并发冲突: 利用PostgreSQL事务和锁机制
- 需求变更: 保持沟通，采用敏捷开发方式

## 8. 团队分工
- 数据库层开发: 1人
- 业务逻辑开发: 1人
- gRPC服务开发: 1人
- 测试与文档: 1人

以上计划根据实际开发情况可能需要调整。