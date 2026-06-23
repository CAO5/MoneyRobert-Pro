import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ROUTES = ROOT / "backend" / "src" / "routes"
MIGRATIONS = ROOT / "backend" / "migrations"

PREFIX = {
    "admin": "/api/v2/admin", "agent_analysis_api": "/api/v2/agent-analysis",
    "agent_simulation": "/api/v2/agents", "ai_analysis": "/api/v2/analysis",
    "ai_chat": "/api/v2/chat", "ai_predictions": "/api/v2/forecasts",
    "ai_providers": "/api/v2/ai-providers", "api_keys": "/api/v2/exchange-credentials",
    "auth": "/api/v2/auth", "auto_trading": "/api/v2/automation",
    "backtest_api": "/api/v2/backtests", "billing": "/api/v2/billing",
    "dashboard": "/api/v2/dashboard", "evolution_api": "/api/v2/evolution",
    "health": "/api/v2/health", "market_data": "/api/v2/market",
    "memory_api": "/api/v2/memory", "news": "/api/v2/news",
    "notifications": "/api/v2/notifications", "paper_trading": "/api/v2/paper",
    "reports": "/api/v2/reports", "sentiment_data": "/api/v2/sentiment",
    "strategies": "/api/v2/strategies", "system_settings": "/api/v2/system",
    "tasks": "/api/v2/jobs", "trading": "/api/v2/execution",
    "validation": "/api/v2/validation",
}


def balanced_calls(text):
    pos = 0
    while True:
        start = text.find(".route(", pos)
        if start < 0:
            return
        i = start + len(".route(")
        depth, string, escape = 1, False, False
        while i < len(text) and depth:
            ch = text[i]
            if string:
                if escape:
                    escape = False
                elif ch == "\\":
                    escape = True
                elif ch == '"':
                    string = False
            else:
                if ch == '"': string = True
                elif ch == "(": depth += 1
                elif ch == ")": depth -= 1
            i += 1
        yield text[start:i]
        pos = i


routes = []
for path in sorted(ROUTES.glob("*.rs")):
    module = path.stem
    if module == "mod":
        continue
    text = path.read_text(encoding="utf-8")
    for call in balanced_calls(text):
        pm = re.search(r'\.route\(\s*"([^"]+)"', call)
        if not pm:
            continue
        route_path = pm.group(1)
        handlers = re.findall(r'\b(get|post|put|delete|patch)\s*\(\s*([A-Za-z0-9_]+)', call)
        for method, handler in handlers:
            routes.append({
                "module": module,
                "method": method.upper(),
                "legacy_path": route_path,
                "v2_path": PREFIX.get(module, f"/api/v2/{module}") + ("" if route_path == "/" else route_path),
                "handler": handler,
            })

tables = []
for path in sorted(MIGRATIONS.glob("*.sql")):
    text = path.read_text(encoding="utf-8")
    for match in re.finditer(r'CREATE TABLE IF NOT EXISTS\s+([A-Za-z0-9_]+)\s*\((.*?)\);', text, re.S | re.I):
        name, body = match.group(1), match.group(2)
        columns = []
        for raw in body.splitlines():
            line = raw.strip().rstrip(",")
            if not line or line.upper().startswith(("PRIMARY KEY", "FOREIGN KEY", "UNIQUE", "CHECK", "CONSTRAINT")):
                continue
            cm = re.match(r'([A-Za-z_][A-Za-z0-9_]*)\s+([^,]+)', line)
            if cm:
                columns.append({"name": cm.group(1), "definition": cm.group(2).strip()})
        tables.append({"migration": path.name, "name": name, "columns": columns})

structs = []
for path in sorted(ROUTES.glob("*.rs")):
    text = path.read_text(encoding="utf-8")
    for match in re.finditer(r'(?:pub\s+)?struct\s+([A-Za-z0-9_]*(?:Request|Response|Query|Params))\s*\{(.*?)\n\}', text, re.S):
        fields = []
        for fm in re.finditer(r'(?:pub\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*:\s*([^,\n]+)', match.group(2)):
            fields.append({"name": fm.group(1), "type": fm.group(2).strip()})
        structs.append({"module": path.stem, "name": match.group(1), "fields": fields})

inventory = {"routes": routes, "tables": tables, "structs": structs}
out = Path(__file__).resolve().parent / "system2_inventory.json"
out.write_text(json.dumps(inventory, ensure_ascii=False, indent=2), encoding="utf-8")
print(f"routes={len(routes)} tables={len(tables)} structs={len(structs)}")
print(out)
