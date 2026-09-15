#!/usr/bin/env bash
#
# Build the tdmelodic evaluation environment (offline accent estimation).
#
# tdmelodic generates accent information for Japanese words using a small
# neural model (BSD-3). musculus uses it offline: the model's output is
# turned into a jpreprocess-format user dictionary, which the frontend loads
# through `JaFrontend::with_user_dictionary` (see
# docs/accent-resources.md). Nothing here is bundled in the repository.
#
# Everything installs under the workspace, because this environment cannot
# write outside it: Python 3.9 comes from uv, MeCab is built from the Debian
# source tarball, and UniDic goes into .mecab/lib/mecab/dic/unidic.
#
# Requirements: network access, a C/C++ toolchain (Xcode command line tools),
# and roughly 1 GB of disk.
#
# Usage:
#   scripts/setup_tdmelodic.sh            # build everything
#   scripts/setup_tdmelodic.sh --verify   # only run the verification
#
# Runtime environment for the tools afterwards:
#   export PATH="$PWD/.mecab/bin:$PATH"
#   export SETUPTOOLS_USE_DISTUTILS=stdlib
#   echo 機械学習 | .venv-tdmelodic39/bin/tdmelodic-s2ya

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

UV=.venv/bin/uv
PY39=.uv-python/cpython-3.9.25-macos-aarch64-none/bin/python3.9
VENV=.venv-tdmelodic39
MECAB_PREFIX="$ROOT/.mecab"
UNIDIC_SRC="$ROOT/.tdmelodic-build/unidic-mecab_kana-accent-2.1.2_src"

verify() {
    export PATH="$MECAB_PREFIX/bin:$PATH"
    export SETUPTOOLS_USE_DISTUTILS=stdlib
    echo "[verify] mecab-config: $(mecab-config --version)"
    echo "[verify] dictionary:"
    ls "$MECAB_PREFIX/lib/mecab/dic/unidic" | head -3 | sed 's/^/    /'
    echo "[verify] tdmelodic:"
    for word in 機械学習 深層学習 IoT; do
        printf '    %-10s -> ' "$word"
        echo "$word" | "$VENV/bin/tdmelodic-s2ya" 2>/dev/null | tail -1
    done
}

if [[ "${1:-}" == "--verify" ]]; then
    verify
    exit 0
fi

# --- 1. Python 3.9 (uv keeps everything inside the workspace) --------------
if [[ ! -x "$PY39" ]]; then
    echo "[1/6] fetching Python 3.9 via uv"
    UV_CACHE_DIR="$ROOT/.uv-cache" UV_PYTHON_INSTALL_DIR="$ROOT/.uv-python" \
        "$UV" python install 3.9
    # uv also tries to symlink into ~/.local/bin, which is not writable here;
    # that failure is harmless because we call the interpreter by path.
fi

# --- 2. environment for tdmelodic ------------------------------------------
# tdmelodic is Chainer-based (development ended in 2019), so it needs an
# older Python and a build without PEP 517 isolation (its setup.py wants
# pkg_resources from the environment's setuptools, not an isolated one).
if [[ ! -x "$VENV/bin/tdmelodic-s2ya" ]]; then
    echo "[2/6] creating the tdmelodic virtualenv"
    "$PY39" -m venv "$VENV"
    "$VENV/bin/pip" install --quiet --upgrade pip
    "$VENV/bin/pip" install --quiet "setuptools<81" wheel Cython "numpy<2"
    "$VENV/bin/pip" install --no-build-isolation chainer
fi

# --- 3. tdmelodic and its remaining dependencies ---------------------------
if [[ ! -d .tdmelodic-reference ]]; then
    echo "[3/6] cloning tdmelodic"
    git clone --depth 1 https://github.com/PKSHATechnology-Research/tdmelodic \
        .tdmelodic-reference
fi
if [[ ! -x "$VENV/bin/tdmelodic-s2ya" ]]; then
    "$VENV/bin/pip" install --quiet mecab-python3 ipadic jaconv \
        python-Levenshtein tqdm regex romkan
    "$VENV/bin/pip" install --quiet --no-build-isolation -e .tdmelodic-reference
fi

# --- 4. MeCab (the build tools UniDic needs) -------------------------------
if [[ ! -x "$MECAB_PREFIX/bin/mecab-config" ]]; then
    echo "[4/6] building MeCab 0.996"
    mkdir -p .tdmelodic-build
    cd .tdmelodic-build
    # taku910/mecab carries no release tarballs; the Debian source is the
    # same 0.996 code and is a stable URL.
    curl -sL "http://deb.debian.org/debian/pool/main/m/mecab/mecab_0.996.orig.tar.gz" \
        -o mecab.tar.gz
    tar xzf mecab.tar.gz
    cd mecab-0.996
    ./configure --prefix="$MECAB_PREFIX" --enable-utf8-only
    make -j4
    make install
    cd "$ROOT"
fi

# --- 5. UniDic with accent information (kana-accent) -----------------------
# The kana-accent edition carries the accent column tdmelodic reads. Licence:
# GPL v2.0 / LGPL v2.1 / modified BSD (see docs/model-licenses.md §6).
if [[ ! -f "$MECAB_PREFIX/lib/mecab/dic/unidic/sys.dic" ]]; then
    echo "[5/6] downloading and building UniDic kana-accent (about 144 MB)"
    mkdir -p .tdmelodic-build
    cd .tdmelodic-build
    curl -sL "https://clrd.ninjal.ac.jp/unidic_archive/cwj/2.1.2/unidic-mecab_kana-accent-2.1.2_src.zip" \
        -o unidic.zip
    unzip -q -o unidic.zip
    cd unidic-mecab_kana-accent-2.1.2_src
    # configure looks for mecab-config on PATH
    PATH="$MECAB_PREFIX/bin:$PATH" \
        ./configure --with-dicdir="$MECAB_PREFIX/lib/mecab/dic/unidic"
    make -j4
    make install
    cd "$ROOT"
fi

# --- 6. verify -------------------------------------------------------------
echo "[6/6] verifying"
verify
echo
echo "done. The pretrained tdmelodic model downloads on first use into"
echo ".tdmelodic-reference/tdmelodic/nn/resource/ (about 1.4 MB)."
