import runpy
import shutil
from pathlib import Path

import pytest

ROOT = Path(__file__).parent.parent
EXAMPLES = sorted((ROOT / "docs_src").glob("*.py"))


@pytest.mark.parametrize("example", EXAMPLES, ids=lambda path: path.stem)
def test_example_prints_documented_output(example, tmp_path, monkeypatch, capsys):
    shutil.copy(ROOT / "docs" / "assets" / "report.pdf", tmp_path)
    monkeypatch.chdir(tmp_path)

    runpy.run_path(str(example))

    expected = example.with_suffix(".out").read_text(encoding="utf-8")
    assert capsys.readouterr().out == expected
