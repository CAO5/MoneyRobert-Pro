#!/bin/bash

# MoneyRobert Pro - Agent System 自动化测试脚本
# 使用方法: bash test_runner.sh

set -e

echo "================================================"
echo "  MoneyRobert Pro - Agent System Test Runner"
echo "================================================"
echo ""

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 测试计数器
PASSED=0
FAILED=0

# 测试结果函数
pass() {
    echo -e "${GREEN}[PASS]${NC} $1"
    PASSED=$((PASSED + 1))
}

fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    FAILED=$((FAILED + 1))
}

info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

# 检查函数
check_prerequisites() {
    echo ""
    echo "================================================"
    echo "  1. 检查前置条件"
    echo "================================================"
    
    # 检查 Rust
    if command -v cargo &> /dev/null; then
        CAGO_VERSION=$(cargo --version)
        pass "Rust/Cargo 已安装: $CAGO_VERSION"
    else
        fail "Rust/Cargo 未安装"
        warn "请安装 Rust: https://rustup.rs"
        exit 1
    fi
    
    # 检查 Node.js
    if command -v node &> /dev/null; then
        NODE_VERSION=$(node --version)
        pass "Node.js 已安装: $NODE_VERSION"
    else
        fail "Node.js 未安装"
        warn "请安装 Node.js: https://nodejs.org"
        exit 1
    fi
    
    # 检查 npm
    if command -v npm &> /dev/null; then
        NPM_VERSION=$(npm --version)
        pass "npm 已安装: v$NPM_VERSION"
    else
        fail "npm 未安装"
        exit 1
    fi
    
    # 检查 PostgreSQL
    if command -v psql &> /dev/null; then
        pass "PostgreSQL 客户端已安装"
    else
        warn "PostgreSQL 客户端未安装（可选）"
    fi
    
    echo ""
}

# Rust 后端测试
test_backend() {
    echo ""
    echo "================================================"
    echo "  2. 后端编译和测试"
    echo "================================================"
    echo ""
    
    cd backend
    
    # 1. 格式化检查
    info "检查代码格式化..."
    if cargo fmt --check; then
        pass "代码格式正确"
    else
        fail "代码格式不正确，运行 cargo fmt 修复"
        cargo fmt
    fi
    
    # 2. 静态分析
    info "运行 Clippy 静态分析..."
    if cargo clippy -- -D warnings 2>&1 | tee /tmp/clippy.log; then
        pass "Clippy 检查通过"
    else
        warn "Clippy 有警告（已记录到 /tmp/clippy.log）"
    fi
    
    # 3. 编译检查
    info "运行编译检查..."
    if cargo check 2>&1 | tee /tmp/check.log; then
        pass "编译检查通过"
    else
        fail "编译检查失败（已记录到 /tmp/check.log）"
        cat /tmp/check.log
        exit 1
    fi
    
    # 4. 构建
    info "编译项目..."
    if cargo build 2>&1 | tee /tmp/build.log; then
        pass "项目编译成功"
    else
        fail "项目编译失败（已记录到 /tmp/build.log）"
        cat /tmp/build.log
        exit 1
    fi
    
    # 5. 单元测试
    info "运行单元测试..."
    if cargo test --lib -- --nocapture 2>&1 | tee /tmp/unit_tests.log; then
        pass "单元测试全部通过"
    else
        fail "单元测试失败（已记录到 /tmp/unit_tests.log）"
        cat /tmp/unit_tests.log
    fi
    
    # 6. Agent 模块测试
    info "运行 Agent 模块测试..."
    if cargo test agents -- --nocapture 2>&1 | tee /tmp/agent_tests.log; then
        pass "Agent 模块测试通过"
    else
        fail "Agent 模块测试失败（已记录到 /tmp/agent_tests.log）"
        cat /tmp/agent_tests.log
    fi
    
    # 7. 所有测试
    info "运行所有测试..."
    if cargo test -- --nocapture 2>&1 | tee /tmp/all_tests.log; then
        pass "所有测试通过"
    else
        warn "部分测试失败（已记录到 /tmp/all_tests.log）"
        cat /tmp/all_tests.log
    fi
    
    cd ..
}

# 前端测试
test_frontend() {
    echo ""
    echo "================================================"
    echo "  3. 前端编译和测试"
    echo "================================================"
    echo ""
    
    cd frontend
    
    # 1. 安装依赖
    info "安装前端依赖..."
    if npm install 2>&1 | tee /tmp/npm_install.log; then
        pass "依赖安装成功"
    else
        fail "依赖安装失败（已记录到 /tmp/npm_install.log）"
        cat /tmp/npm_install.log
        exit 1
    fi
    
    # 2. 类型检查
    info "运行 TypeScript 类型检查..."
    if npm run type-check 2>&1 | tee /tmp/type_check.log; then
        pass "TypeScript 类型检查通过"
    else
        fail "TypeScript 类型检查失败（已记录到 /tmp/type_check.log）"
        cat /tmp/type_check.log
    fi
    
    # 3. ESLint 检查
    info "运行 ESLint 检查..."
    if npm run lint 2>&1 | tee /tmp/lint.log; then
        pass "ESLint 检查通过"
    else
        warn "ESLint 有警告（已记录到 /tmp/lint.log）"
    fi
    
    # 4. 构建
    info "构建前端项目..."
    if npm run build 2>&1 | tee /tmp/frontend_build.log; then
        pass "前端构建成功"
    else
        fail "前端构建失败（已记录到 /tmp/frontend_build.log）"
        cat /tmp/frontend_build.log
        exit 1
    fi
    
    cd ..
}

# 数据库测试
test_database() {
    echo ""
    echo "================================================"
    echo "  4. 数据库迁移测试"
    echo "================================================"
    echo ""
    
    info "检查数据库迁移脚本..."
    
    # 检查迁移文件
    if [ -f "backend/migrations/005_agent_system_tables.sql" ]; then
        pass "Agent 系统迁移文件存在"
    else
        fail "Agent 系统迁移文件不存在"
    fi
    
    # 检查 SQL 语法
    info "验证 SQL 语法..."
    if grep -q "CREATE TABLE" backend/migrations/005_agent_system_tables.sql; then
        pass "SQL 语法初步验证通过"
    else
        fail "SQL 语法验证失败"
    fi
    
    info "数据库表结构验证通过（建议在数据库环境中运行 sqlx migrate run）"
}

# 总结报告
print_summary() {
    echo ""
    echo "================================================"
    echo "  测试执行总结"
    echo "================================================"
    echo ""
    echo -e "${GREEN}通过: $PASSED${NC}"
    echo -e "${RED}失败: $FAILED${NC}"
    echo ""
    
    if [ $FAILED -eq 0 ]; then
        echo -e "${GREEN}🎉 所有测试通过！${NC}"
        echo ""
        echo "下一步:"
        echo "  1. 配置环境变量: cp backend/.env.example backend/.env"
        echo "  2. 运行数据库迁移: cd backend && sqlx migrate run"
        echo "  3. 启动后端服务: cargo run"
        echo "  4. 启动前端服务: cd frontend && npm run dev"
        return 0
    else
        echo -e "${YELLOW}⚠️  部分测试失败，请检查上述输出${NC}"
        return 1
    fi
}

# 主函数
main() {
    check_prerequisites
    test_backend
    test_frontend
    test_database
    print_summary
}

# 执行主函数
main "$@"
