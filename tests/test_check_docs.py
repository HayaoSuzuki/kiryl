"""Failures that the documentation gate must catch before changes merge."""

from pathlib import Path

import pytest

from tools.check_docs import check_repository, validate_okf


def bundle(tmp_path: Path) -> Path:
    root = tmp_path / "docs/development/knowledge"
    root.mkdir(parents=True)
    (root / "index.md").write_text(
        '---\nokf_version: "0.2"\n---\n# Knowledge\n\n[Rule](rule.md)\n'
    )
    (root / "rule.md").write_text(
        "---\ntype: Decision\nstatus: draft\ncustom: keep-me\nsources:\n"
        "  - id: evidence\n    resource: ../evidence.md\n---\n"
        "# Rule\n\nRead the evidence.[^evidence]\n\n"
        "[^evidence]: [Evidence](../evidence.md).\n"
    )
    (root.parent / "evidence.md").write_text("# Evidence\n")
    return root


def test_accepts_valid_bundle_and_preserves_unknown_metadata(tmp_path: Path) -> None:
    root = bundle(tmp_path)
    original = (root / "rule.md").read_bytes()
    assert check_repository(tmp_path) == []
    assert (root / "rule.md").read_bytes() == original


@pytest.mark.parametrize(
    "content",
    [
        "# Missing frontmatter\n",
        "---\n# type: Decision\n---\n# A comment is not metadata\n",
        "---\ntype: []\n---\n# Invalid type\n",
        "---\ntype: ' '\n---\n# Empty type\n",
        "---\n- Decision\n---\n# Not a mapping\n",
        "---\ntype: [\n---\n# Malformed YAML\n",
        "---\ntype: Decision\n# Unterminated header\n",
    ],
)
def test_rejects_malformed_concepts(tmp_path: Path, content: str) -> None:
    root = bundle(tmp_path)
    (root / "rule.md").write_text(content)
    errors = check_repository(tmp_path)
    assert errors
    assert any("rule.md" in error for error in errors)


@pytest.mark.parametrize("name", ["nested/index.md", "log.md"])
def test_reserved_files_cannot_hide_concepts(tmp_path: Path, name: str) -> None:
    root = bundle(tmp_path)
    page = root / name
    page.parent.mkdir(exist_ok=True)
    page.write_text("---\ntype: Decision\n---\n# Reserved\n")
    assert any(name in error for error in validate_okf(root))


def test_requires_explicit_bundle_version(tmp_path: Path) -> None:
    root = bundle(tmp_path)
    (root / "index.md").write_text("# Knowledge\n[Rule](rule.md)\n")
    assert any("index.md" in error for error in validate_okf(root))


def test_missing_entry_point_fails_with_a_diagnostic(tmp_path: Path) -> None:
    root = bundle(tmp_path)
    (root / "index.md").unlink()
    assert any("index.md" in error for error in check_repository(tmp_path))


def test_missing_source_and_orphan_are_reported(tmp_path: Path) -> None:
    root = bundle(tmp_path)
    (root.parent / "evidence.md").unlink()
    (root / "orphan.md").write_text((root / "rule.md").read_text())
    errors = check_repository(tmp_path)
    assert any("evidence.md" in error for error in errors)
    assert any("orphan.md" in error for error in errors)


def test_checks_user_docs_and_agent_entry_links(tmp_path: Path) -> None:
    bundle(tmp_path)
    user = tmp_path / "docs/user"
    user.mkdir()
    (user / "index.md").write_text("[Quick start](absent.md)\n")
    (tmp_path / "AGENTS.md").write_text("[Rules](missing-rules.md)\n")
    errors = check_repository(tmp_path)
    assert any("absent.md" in error for error in errors)
    assert any("missing-rules.md" in error for error in errors)


def test_ignores_external_urls_and_links_in_examples(tmp_path: Path) -> None:
    bundle(tmp_path)
    (tmp_path / "README.md").write_text(
        "[External](https://example.org/unfetched)\n"
        "```markdown\n[Example](not-a-real-file.md)\n```\n"
    )
    assert check_repository(tmp_path) == []


def test_source_ids_require_matching_citations(tmp_path: Path) -> None:
    root = bundle(tmp_path)
    page = root / "rule.md"
    page.write_text(page.read_text().replace("id: evidence", "id: uncited"))
    assert any("uncited" in error for error in validate_okf(root))


def test_duplicate_source_ids_fail(tmp_path: Path) -> None:
    root = bundle(tmp_path)
    page = root / "rule.md"
    page.write_text(
        page.read_text().replace(
            "    resource: ../evidence.md\n---",
            "    resource: ../evidence.md\n  - id: evidence\n"
            "    resource: ../evidence.md\n---",
        )
    )
    assert any("duplicate" in error for error in validate_okf(root))


def test_definition_alone_does_not_attribute_a_claim(tmp_path: Path) -> None:
    root = bundle(tmp_path)
    page = root / "rule.md"
    page.write_text(
        page.read_text().replace("Read the evidence.[^evidence]", "No attribution.")
    )
    assert any("evidence" in error for error in validate_okf(root))


@pytest.mark.parametrize(
    "replacement",
    [
        "sources: invalid",
        "sources: [invalid]",
        "sources: [{id: evidence}]",
        "sources: [{id: evidence, resource: []}]",
        "sources: [{id: ' ', resource: ../evidence.md}]",
    ],
)
def test_malformed_sources_produce_diagnostics(
    tmp_path: Path, replacement: str
) -> None:
    root = bundle(tmp_path)
    page = root / "rule.md"
    page.write_text(
        page.read_text().replace(
            "sources:\n  - id: evidence\n    resource: ../evidence.md", replacement
        )
    )
    assert any("rule.md" in error for error in check_repository(tmp_path))
