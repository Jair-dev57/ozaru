"""The `ozaru` command, run as `python -m ozaru`."""

import json
import subprocess
import sys
from pathlib import Path

import pytest

import ozaru

FILES = Path(__file__).parent
PDF = FILES / "factura.pdf"


def run(*args, stdin=None, check=True):
    done = subprocess.run(
        [sys.executable, "-m", "ozaru", *map(str, args)],
        input=stdin,
        capture_output=True,
        check=False,
    )
    if check:
        assert done.returncode == 0, done.stderr.decode()
    return done


def test_text():
    out = run("text", PDF).stdout.decode()
    assert out == ozaru.extract_text(PDF) + "\n"
    pages = run("text", PDF, "--pages", "2-3").stdout.decode()
    assert pages == "\x0c".join(ozaru.extract_text(PDF).split("\x0c")[1:]) + "\n"


def test_count_and_stdin():
    assert run("count", PDF).stdout == b"3\n"
    assert run("count", "-", stdin=PDF.read_bytes()).stdout == b"3\n"


def test_info_is_json():
    info = json.loads(run("info", PDF).stdout)
    assert info["pages"] == 3
    assert info["metadata"]["title"] == "Documento de prueba"
    assert len(info["page_sizes"]) == 3


def test_words_tab_separated_and_json():
    rows = run("words", PDF).stdout.decode().splitlines()
    assert len(rows) == len(ozaru.extract_words(PDF))
    assert rows[0].split("\t")[3] == ozaru.extract_words(PDF)[0].text
    words = json.loads(run("words", PDF, "--json").stdout)
    assert words[0]["page"] == 1 and "x0" in words[0]


def test_blocks_annotations_fields():
    assert run("blocks", PDF).stdout
    assert json.loads(run("annotations", PDF, "--json").stdout) == []
    assert json.loads(run("fields", PDF, "--json").stdout) == []


def test_render(tmp_path):
    out = tmp_path / "page.png"
    run("render", PDF, 1, "-o", out, "--scale", "0.5")
    assert out.read_bytes() == ozaru.render_page(PDF, 1, scale=0.5)
    assert run("render", PDF, 1, "-o", "-").stdout == ozaru.render_page(PDF, 1)


def test_edit_commands(tmp_path):
    run("rotate", PDF, 90, "-o", tmp_path / "r.pdf", "--pages", "1")
    assert ozaru.page_info(tmp_path / "r.pdf")[0].rotation == 90
    run("extract-pages", PDF, "3,1", "-o", tmp_path / "e.pdf")
    assert ozaru.page_count(tmp_path / "e.pdf") == 2
    run("split", PDF, "-o", tmp_path / "parts")
    assert len(list((tmp_path / "parts").glob("*.pdf"))) == 3
    run("merge", PDF, PDF, "-o", tmp_path / "m.pdf")
    assert ozaru.page_count(tmp_path / "m.pdf") == 6
    run("encrypt", PDF, "-o", tmp_path / "x.pdf", "--user-password", "a")
    assert run("count", tmp_path / "x.pdf", "-p", "a").stdout == b"3\n"
    assert run("text", tmp_path / "x.pdf", "-p", "a").stdout


def test_images(tmp_path):
    brochure = FILES.parent / "docs" / "assets" / "brochure.pdf"
    expected = [image for image in ozaru.extract_images(brochure) if image.data is not None]
    assert expected
    out = run("images", brochure, "-o", tmp_path / "img")
    assert len(out.stdout.splitlines()) == len(expected)
    assert len(list((tmp_path / "img").iterdir())) == len(expected)


def test_errors_exit_with_one_and_a_message():
    done = run("count", "missing.pdf", check=False)
    assert done.returncode == 1
    assert done.stderr.startswith(b"ozaru: ")
    done = run("text", FILES / "password.pdf", check=False)
    assert done.returncode == 1
    assert b"password" in done.stderr


def test_bad_arguments():
    assert run(check=False).returncode == 2
    assert run("text", PDF, "--pages", "x", check=False).returncode == 2


def test_closed_pipe_is_quiet():
    done = subprocess.run(
        f"{sys.executable} -m ozaru words {PDF} | head -c 1",
        shell=True,
        capture_output=True,
    )
    assert done.returncode == 0


def test_version():
    assert run("--version").stdout.startswith(b"ozaru ")
