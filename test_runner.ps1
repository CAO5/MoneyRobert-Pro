# MoneyRobert Pro - Agent System 自动化测试脚本 (PowerShell)
# 使用方法: .\test_runner.ps1

param(
    [switch]$SkipFrontend,
    [switch]$SkipBackend,
    [switch]$Verbose
)

$ErrorActionPreference = "Continue"

Write-Host ""
Write-Host "================================================" -ForegroundColor Cyan
Write-Host "  MoneyRobert Pro - Agent System Test Runner" -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""

# 测试计数器
$PASSED = 0
$FAILED = 0
$WARNINGS = 0

# 测试结果函数
function Pass {
    param([string]$Message)
    Write-Host "[PASS] " -ForegroundColor Green -NoNewline
    Write-Host $Message
    script:PASSED++
}

function Fail {
    param([string]$Message)
    Write-Host "[FAIL] " -ForegroundColor Red -NoNewline
    Write-Host $Message
    script:FAILED++
}

function Info {
    param([string]$Message)
    Write-Host "[INFO] " -ForegroundColor Blue -NoNewline
    Write-Host $Message
}

function Warn {
    param([string]$Message)
    Write-Host "[WARN] " -ForegroundColor Yellow -NoNewline
    Write-Host $Message
    script:WARNINGS++
}

# 前置条件检查
function Check-Prerequisites {
    Write-Host ""
    Write-Host "================================================" -ForegroundColor Cyan
    Write-Host "  1. 检查前置条件" -ForegroundColor Cyan
    Write-Host "================================================" -ForegroundColor Cyan
    Write-Host ""
    
    # 检查 Rust
    $cargoVersion = cargo --version 2>$null
    if ($LASTEXITCODE -eq 0) {
        Pass "Rust/Cargo 已安装: $cargoVersion"
    } else {
        Fail "Rust/Cargo 未安装"
        Warn "请安装 Rust: https://rustup.rs"
        exit 1
    }
    
    # 检查 Node.js
    $nodeVersion = node --version 2>$null
    if ($LASTEXITCODE -eq 0) {
        Pass "Node.js 已安装: $nodeVersion"
    } else {
        Fail "Node.js 未安装"
        Warn "请安装 Node.js: https://nodejs.org"
        exit 1
    }
    
    # 检查 npm
    $npmVersion = npm --version 2>$null
    if ($LASTEXITCODE -eq 0) {
        Pass "npm 已安装: v$npmVersion"
    } else {
        Fail "npm 未安装"
        exit 1
    }
    
    # 检查 PostgreSQL
    $psqlVersion = psql --version 2>$null
    if ($LASTEXITCODE -eq 0) {
        Pass "PostgreSQL 客户端已安装: $psqlVersion"
    } else {
        Warn "PostgreSQL 客户端未安装（可选）"
    }
    
    # 检查 SQLx
    $sqlxVersion = sqlx --version 2>$null
    if ($LASTEXITCODE -eq 0) {
        Pass "SQLx 已安装: $sqlxVersion"
    } else {
        Warn "SQLx CLI 未安装（可选，用于数据库迁移）"
    }
}

# 后端测试
function Test-Backend {
    Write-Host ""
    Write-Host "================================================" -ForegroundColor Cyan
    Write-Host "  2. 后端编译和测试" -ForegroundColor Cyan
    Write-Host "================================================" -ForegroundColor Cyan
    Write-Host ""
    
    Set-Location backend
    
    try {
        # 1. 格式化检查
        Info "检查代码格式化..."
        cargo fmt --check 2>$null
        if ($LASTEXITCODE -eq 0) {
            Pass "代码格式正确"
        } else {
            Fail "代码格式不正确"
            Info "运行 cargo fmt 修复..."
            cargo fmt
        }
        
        # 2. 编译检查
        Info "运行编译检查..."
        cargo check 2>&1 | Tee-Object -FilePath "$env:TEMP\cargo_check.log"
        if ($LASTEXITCODE -eq 0) {
            Pass "编译检查通过"
        } else {
            Fail "编译检查失败（查看 $env:TEMP\cargo_check.log）"
            if (-not $Verbose) { Get-Content "$env:TEMP\cargo_check.log" | Select-Object -Last 20 }
        }
        
        # 3. 构建
        Info "编译项目（这可能需要几分钟）..."
        cargo build 2>&1 | Tee-Object -FilePath "$env:TEMP\cargo_build.log"
        if ($LASTEXITCODE -eq 0) {
            Pass "项目编译成功"
        } else {
            Fail "项目编译失败（查看 $env:TEMP\cargo_build.log）"
            if (-not $Verbose) { Get-Content "$env:TEMP\cargo_build.log" | Select-Object -Last 20 }
        }
        
        # 4. 单元测试
        Info "运行单元测试..."
        cargo test --lib -- --nocapture 2>&1 | Tee-Object -FilePath "$env:TEMP\cargo_test_lib.log"
        if ($LASTEXITCODE -eq 0) {
            Pass "单元测试全部通过"
        } else {
            Fail "单元测试失败（查看 $env:TEMP\cargo_test_lib.log）"
        }
        
        # 5. Agent 模块测试
        Info "运行 Agent 模块测试..."
        cargo test agents -- --nocapture 2>&1 | Tee-Object -FilePath "$env:TEMP\cargo_test_agents.log"
        if ($LASTEXITCODE -eq 0) {
            Pass "Agent 模块测试通过"
        } else {
            Fail "Agent 模块测试失败（查看 $env:TEMP\cargo_test_agents.log）"
        }
        
        # 6. 所有测试
        Info "运行所有测试..."
        cargo test -- --nocapture 2>&1 | Tee-Object -FilePath "$env:TEMP\cargo_test_all.log"
        if ($LASTEXITCODE -eq 0) {
            Pass "所有测试通过"
        } else {
            Warn "部分测试失败（查看 $env:TEMP\cargo_test_all.log）"
        }
        
    } finally {
        Set-Location ..
    }
}

# 前端测试
function Test-Frontend {
    if ($SkipFrontend) {
        Write-Host ""
        Write-Host "跳过前端测试（-SkipFrontend 参数）" -ForegroundColor Yellow
        return
    }
    
    Write-Host ""
    Write-Host "================================================" -ForegroundColor Cyan
    Write-Host "  3. 前端编译和测试" -ForegroundColor Cyan
    Write-Host "================================================" -ForegroundColor Cyan
    Write-Host ""
    
    Set-Location frontend
    
    try {
        # 1. 安装依赖
        Info "安装前端依赖（这可能需要几分钟）..."
        npm install 2>&1 | Tee-Object -FilePath "$env:TEMP\npm_install.log"
        if ($LASTEXITCODE -eq 0) {
            Pass "依赖安装成功"
        } else {
            Fail "依赖安装失败（查看 $env:TEMP\npm_install.log）"
        }
        
        # 2. 类型检查
        Info "运行 TypeScript 类型检查..."
        npm run type-check 2>&1 | Tee-Object -FilePath "$env:TEMP\npm_type_check.log"
        if ($LASTEXITCODE -eq 0) {
            Pass "TypeScript 类型检查通过"
        } else {
            Fail "TypeScript 类型检查失败（查看 $env:TEMP\npm_type_check.log）"
        }
        
        # 3. ESLint 检查
        Info "运行 ESLint 检查..."
        npm run lint 2>&1 | Tee-Object -FilePath "$env:TEMP\npm_lint.log"
        if ($LASTEXITCODE -eq 0) {
            Pass "ESLint 检查通过"
        } else {
            Warn "ESLint 有警告（查看 $env:TEMP\npm_lint.log）"
        }
        
        # 4. 构建
        Info "构建前端项目..."
        npm run build 2>&1 | Tee-Object -FilePath "$env:TEMP\npm_build.log"
        if ($LASTEXITCODE -eq 0) {
            Pass "前端构建成功"
        } else {
            Fail "前端构建失败（查看 $env:TEMP\npm_build.log）"
        }
        
    } finally {
        Set-Location ..
    }
}

# 数据库验证
function Test-Database {
    Write-Host ""
    Write-Host "================================================" -ForegroundColor Cyan
    Write-Host "  4. 数据库迁移验证" -ForegroundColor Cyan
    Write-Host "================================================" -ForegroundColor Cyan
    Write-Host ""
    
    # 检查迁移文件
    if (Test-Path "backend\migrations\005_agent_system_tables.sql") {
        Pass "Agent 系统迁移文件存在"
    } else {
        Fail "Agent 系统迁移文件不存在"
    }
    
    # 统计表数量
    $tableCount = (Select-String -Path "backend\migrations\005_agent_system_tables.sql" -Pattern "CREATE TABLE" | Measure-Object).Count
    Info "迁移文件定义了 $tableCount 个表"
    
    if ($tableCount -ge 14) {
        Pass "表结构完整（$tableCount 个表）"
    } else {
        Fail "表结构不完整（期望至少 14 个表，实际 $tableCount 个）"
    }
    
    # 验证关键表
    $requiredTables = @(
        "ai_simulation_configs",
        "ai_simulation_trades",
        "agent_debate_sessions",
        "promotion_audits",
        "autonomous_decision_logs"
    )
    
    foreach ($table in $requiredTables) {
        if (Select-String -Path "backend\migrations\005_agent_system_tables.sql" -Pattern "CREATE TABLE.*$table") {
            Pass "关键表存在: $table"
        } else {
            Fail "关键表缺失: $table"
        }
    }
}

# 生成报告
function Print-Summary {
    Write-Host ""
    Write-Host "================================================" -ForegroundColor Cyan
    Write-Host "  测试执行总结" -ForegroundColor Cyan
    Write-Host "================================================" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "通过: $PASSED" -ForegroundColor Green
    Write-Host "失败: $FAILED" -ForegroundColor Red
    Write-Host "警告: $WARNINGS" -ForegroundColor Yellow
    Write-Host ""
    
    if ($FAILED -eq 0) {
        Write-Host "所有核心测试通过！" -ForegroundColor Green
        Write-Host ""
        Write-Host "下一步操作：" -ForegroundColor Cyan
        Write-Host "  1. 配置环境变量: Copy backend\.env.example backend\.env"
        Write-Host "  2. 运行数据库迁移: cd backend; sqlx migrate run"
        Write-Host "  3. 启动后端服务: cd backend; cargo run"
        Write-Host "  4. 启动前端服务: cd frontend; npm run dev"
        return 0
    } else {
        Write-Host "部分测试失败，请检查上述输出" -ForegroundColor Yellow
        return 1
    }
}

# 主函数
function Main {
    try {
        Check-Prerequisites
        
        if (-not $SkipBackend) {
            Test-Backend
        } else {
            Write-Host "跳过后端测试（-SkipBackend 参数）" -ForegroundColor Yellow
        }
        
        Test-Frontend
        Test-Database
        Print-Summary
        
    } catch {
        Write-Host ""
        Write-Host "测试执行过程中发生错误: $_" -ForegroundColor Red
        Write-Host $_.ScriptStackTrace -ForegroundColor Red
        exit 1
    }
}

# 执行主函数
Main
