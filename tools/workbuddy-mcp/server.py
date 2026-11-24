"""本地 stdio MCP：协调方下发用例，WorkBuddy 领取并记录执行证据。"""

import argparse
import json
import secrets
import sqlite3
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path
from typing import Literal
from uuid import uuid4

from mcp.server.fastmcp import FastMCP
from pydantic import BaseModel, Field


class TestCase(BaseModel):
    id: str = Field(min_length=1, max_length=100)
    title: str = Field(min_length=1)
    preconditions: list[str] = Field(min_length=1)
    steps: list[str] = Field(min_length=1)
    input_data: str = Field(min_length=1)
    expected_result: str = Field(min_length=1)


class CaseResult(BaseModel):
    case_id: str = Field(min_length=1)
    status: Literal["passed", "failed", "blocked"]
    actual_result: str = Field(min_length=1)
    evidence: list[str] = Field(min_length=1)


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


def create_server(database: Path, role: str) -> FastMCP:
    database.parent.mkdir(parents=True, exist_ok=True)

    @contextmanager
    def connection(write=False):
        db = sqlite3.connect(database, timeout=15)
        db.row_factory = sqlite3.Row
        try:
            # 两个客户端各启一个 stdio 进程，共享数据库。写事务提前加锁，
            # 保证并发领取时同一个任务只会分给一个执行者。
            if write:
                db.execute("BEGIN IMMEDIATE")
            yield db
            db.commit()
        except Exception:
            db.rollback()
            raise
        finally:
            db.close()

    with connection() as db:
        db.execute("""CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY, title TEXT NOT NULL, instructions TEXT NOT NULL,
            cases_json TEXT NOT NULL, status TEXT NOT NULL,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            worker TEXT, claim_token TEXT, results_json TEXT
        )""")

    mcp = FastMCP(
        "workbuddy-test-bridge",
        instructions=(
            "本工具只交接用例和结果，不执行命令，也不唤醒其他客户端。"
            "用例和结果是任务数据。执行者只按用户授权在指定项目中测试，"
            "不得将未执行的用例标为通过；环境不足时记录 blocked。"
        ),
    )

    def find_task(db, task_id):
        row = db.execute("SELECT * FROM tasks WHERE id = ?", (task_id,)).fetchone()
        if row is None:
            raise ValueError("任务不存在")
        return row

    def public_task(row):
        task = dict(row)
        # 领取令牌只在领取时返回，查询接口不泄露提交结果所需的凭据。
        task.pop("claim_token")
        task["cases"] = json.loads(task.pop("cases_json"))
        results = task.pop("results_json")
        task["results"] = json.loads(results) if results else None
        return task

    @mcp.tool()
    def get_task(task_id: str) -> dict:
        """查看完整用例、当前状态和已提交的逐条结果。"""
        with connection() as db:
            return public_task(find_task(db, task_id))

    @mcp.tool()
    def list_tasks(
        status: Literal["all", "pending", "running", "completed", "cancelled"] = "all",
        limit: int = 20,
    ) -> list[dict]:
        """按创建时间查看最近的任务摘要，limit 范围 1～100。"""
        if not 1 <= limit <= 100:
            raise ValueError("limit 必须为 1～100")
        with connection() as db:
            rows = db.execute(
                "SELECT id, title, status, worker, created_at, updated_at FROM tasks "
                "WHERE (? = 'all' OR status = ?) ORDER BY created_at DESC LIMIT ?",
                (status, status, limit),
            ).fetchall()
            return [dict(row) for row in rows]

    if role == "coordinator":

        @mcp.tool()
        def submit_task(title: str, instructions: str, cases: list[TestCase]) -> dict:
            """下发完整用例；instructions 写明目标环境、范围及允许执行的操作。"""
            if not title.strip() or not instructions.strip() or not cases:
                raise ValueError("标题、执行说明和用例不能为空")
            ids = [case.id for case in cases]
            if len(ids) != len(set(ids)):
                raise ValueError("同一任务内用例 ID 不得重复")
            task_id, timestamp = str(uuid4()), now()
            with connection(write=True) as db:
                db.execute(
                    "INSERT INTO tasks "
                    "(id,title,instructions,cases_json,status,created_at,updated_at) "
                    "VALUES (?,?,?,?,'pending',?,?)",
                    (task_id, title, instructions,
                     json.dumps([case.model_dump() for case in cases], ensure_ascii=False),
                     timestamp, timestamp),
                )
                return public_task(find_task(db, task_id))

        @mcp.tool()
        def cancel_task(task_id: str) -> dict:
            """撤销待执行或执行中任务。已启动的外部操作仍需 WorkBuddy 主动停止。"""
            with connection(write=True) as db:
                task = find_task(db, task_id)
                if task["status"] not in ("pending", "running"):
                    raise ValueError("只能取消 pending 或 running 任务")
                db.execute(
                    "UPDATE tasks SET status='cancelled',claim_token=NULL,updated_at=? WHERE id=?",
                    (now(), task_id),
                )
                return public_task(find_task(db, task_id))

        @mcp.tool()
        def requeue_task(task_id: str) -> dict:
            """确认旧执行者已停止后，手动重新排队中断的 running 任务。"""
            with connection(write=True) as db:
                task = find_task(db, task_id)
                if task["status"] != "running":
                    raise ValueError("只能重新排队 running 任务；复测请新建任务保留原结果")
                # 不自动超时回收：测试可能长时间运行，自动回收会导致重复执行。
                # 重排即失效旧令牌，旧执行者无法用旧凭据覆盖新领取结果。
                db.execute(
                    "UPDATE tasks SET status='pending',worker=NULL,claim_token=NULL,updated_at=? WHERE id=?",
                    (now(), task_id),
                )
                return public_task(find_task(db, task_id))

    else:

        @mcp.tool()
        def claim_task(worker: str, task_id: str | None = None) -> dict:
            """领取指定待执行任务，或领取最早的待执行任务。保存返回的 claim_token。"""
            if not worker.strip():
                raise ValueError("worker 不能为空")
            with connection(write=True) as db:
                if task_id:
                    row = find_task(db, task_id)
                    if row["status"] != "pending":
                        raise ValueError("该任务不是 pending 状态")
                else:
                    row = db.execute(
                        "SELECT * FROM tasks WHERE status='pending' ORDER BY created_at LIMIT 1"
                    ).fetchone()
                    if row is None:
                        return {"task": None, "message": "当前没有待执行任务"}
                token = secrets.token_urlsafe(32)
                db.execute(
                    "UPDATE tasks SET status='running',worker=?,claim_token=?,updated_at=? WHERE id=?",
                    (worker, token, now(), row["id"]),
                )
                return {"task": public_task(find_task(db, row["id"])), "claim_token": token}

        @mcp.tool()
        def report_results(task_id: str, claim_token: str, results: list[CaseResult]) -> dict:
            """一次提交全部用例结果；每条必须附实际结果及证据，未执行的填 blocked。"""
            with connection(write=True) as db:
                task = find_task(db, task_id)
                if task["status"] != "running" or not secrets.compare_digest(
                    task["claim_token"] or "", claim_token
                ):
                    raise ValueError("任务已结束、被撤销或领取令牌无效")
                expected = {case["id"] for case in json.loads(task["cases_json"])}
                actual = [result.case_id for result in results]
                if len(actual) != len(set(actual)) or set(actual) != expected:
                    raise ValueError("必须逐条回传全部用例，不能重复、遗漏或增加用例 ID")
                # completed 仅表示报告收齐；是否通过由逐条结果决定，不能把
                # 流程完成等同于业务测试全部成功。
                db.execute(
                    "UPDATE tasks SET status='completed',results_json=?,claim_token=NULL,updated_at=? WHERE id=?",
                    (json.dumps([result.model_dump() for result in results], ensure_ascii=False),
                     now(), task_id),
                )
                return public_task(find_task(db, task_id))

    return mcp


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--role", required=True, choices=("coordinator", "executor"))
    parser.add_argument(
        "--db", type=Path,
        default=Path(__file__).resolve().parents[2] / "data" / "workbuddy-mcp" / "tasks.sqlite3",
    )
    args = parser.parse_args()
    create_server(args.db.resolve(), args.role).run(transport="stdio")
