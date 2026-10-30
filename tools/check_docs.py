"""Check repository documentation and the development OKF bundle, read-only."""

import re
import sys
from pathlib import Path

import yaml

from tools.knowledge_references import (
    inventory,
    local_target,
    markdown_targets,
    unfenced_lines,
)


def validate_okf(root: Path) -> list[str]:
    """Check OKF structure plus the repository's source-citation conventions."""
    errors = []
    if not (root / "index.md").is_file():
        return [f"{root / 'index.md'}: missing OKF entry point"]
    for page in sorted(root.rglob("*.md")):
        text = page.read_text(encoding="utf-8")
        header = text.startswith("---\n")
        reserved = page.name in {"index.md", "log.md"}
        if reserved and page != root / "index.md":
            if header:
                errors.append(f"{page}: reserved files must not have frontmatter")
            continue
        try:
            if not header:
                raise ValueError("missing frontmatter")
            front, body = text[4:].split("\n---\n", 1)
            meta = yaml.safe_load(front)
            if not isinstance(meta, dict):
                raise TypeError("frontmatter must be a mapping")
            if page == root / "index.md":
                if meta != {"okf_version": "0.2"}:
                    raise ValueError('root index must declare only okf_version: "0.2"')
                continue
            if not isinstance(meta.get("type"), str) or not meta["type"].strip():
                raise ValueError("type must be a nonempty string")
            sources = meta.get("sources", [])
            if not isinstance(sources, list):
                raise TypeError("sources must be a list")
            ids = set()
            claim_text = "\n".join(
                line
                for line in unfenced_lines(body)
                if not re.match(r"^\s{0,3}\[\^[^\]]+\]:", line)
            )
            for source in sources:
                if not isinstance(source, dict):
                    raise TypeError("each source must be a mapping")
                resource = source.get("resource")
                if not isinstance(resource, str) or not resource.strip():
                    raise ValueError("source resource must be a nonempty string")
                source_id = source.get("id")
                if not isinstance(source_id, str) or not source_id.strip():
                    raise ValueError("repository sources require a nonempty id")
                if source_id in ids:
                    raise ValueError(f"duplicate source id: {source_id}")
                ids.add(source_id)
                if f"[^{source_id}]" not in claim_text or not re.search(
                    rf"^\[\^{re.escape(source_id)}\]:", body, re.MULTILINE
                ):
                    raise ValueError(f"source {source_id} requires a matching footnote")
        except (TypeError, ValueError, yaml.YAMLError) as error:
            errors.append(f"{page}: {error}")
    return errors


def check_repository(root: Path) -> list[str]:
    """Check bundle navigation and local links, including user and agent entries."""
    root = root.resolve()
    knowledge = root / "docs/development/knowledge"
    errors = validate_okf(knowledge)
    if errors:
        return errors
    references = inventory(knowledge)
    for kind in ("missing_markdown", "missing_sources"):
        for row in references[kind]:
            errors.append(
                f"{knowledge / row['page']}: missing {row.get('target', row.get('resource'))}"
            )
    for page in references["unreachable_pages"]:
        errors.append(f"{knowledge / page}: unreachable from the OKF index")
    pages = sorted((root / "docs").rglob("*.md"))
    pages.extend(
        page for name in ("README.md", "AGENTS.md") if (page := root / name).is_file()
    )
    for page in pages:
        if page.is_relative_to(knowledge):
            continue  # The inventory owns bundle-relative paths.
        text = page.read_text(encoding="utf-8")
        for target in markdown_targets(text):
            resolved = local_target(page, target, root)
            if resolved is not None and not resolved.exists():
                errors.append(f"{page}: missing link target {target}")
    return errors


def main() -> int:
    errors = check_repository(Path.cwd())
    for error in errors:
        print(error, file=sys.stderr)
    if errors:
        return 1
    print("Documentation links, OKF structure, citations, and navigation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
