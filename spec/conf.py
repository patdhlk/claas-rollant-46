import json
from pathlib import Path

project = "claas-rollant-46 — Specification"
extensions = ["sphinx_needs"]
exclude_patterns = ["_build", ".venv"]
source_suffix = {".rst": "restructuredtext"}

needs_from_toml = "../ubproject.toml"

needs_schema_validation_enabled = True
with (Path(__file__).parent / "schemas.json").open("r", encoding="utf-8") as _fh:
    needs_schema_definitions = json.load(_fh)

html_theme = "furo"
