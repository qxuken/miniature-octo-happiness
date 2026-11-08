#!/usr/bin/env python3
"""Compile and run C/C++ consumers against a native MSVC-built Windows DLL."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def compiler_environment(directory):
    if shutil.which('cl.exe'):
        return os.environ.copy()
    installer = Path(os.environ.get('ProgramFiles(x86)', r'C:\Program Files (x86)')) / 'Microsoft Visual Studio/Installer/vswhere.exe'
    installation = subprocess.check_output(
        [str(installer), '-latest', '-products', '*', '-requires', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
         '-property', 'installationPath'], text=True).strip()
    if not installation:
        raise RuntimeError('Visual Studio C/C++ build tools are required')
    vcvars = Path(installation) / 'VC/Auxiliary/Build/vcvarsall.bat'
    batch = directory / 'compiler-env.cmd'
    batch.write_text(f'@echo off\ncall "{vcvars}" x64 >nul\nif errorlevel 1 exit /b 1\nset\n')
    output = subprocess.check_output(['cmd.exe', '/d', '/u', '/c', str(batch)]).decode('utf-16-le')
    return dict(line.split('=', 1) for line in output.splitlines() if '=' in line and not line.startswith('='))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--library', type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != 'win32':
        parser.error('Run this native consumer check on x86_64 Windows with MSVC build tools')
    library = args.library.resolve()
    imports = library.with_suffix('.dll.lib')
    if not library.is_file() or not imports.is_file():
        parser.error('Both the DLL and its .dll.lib import library are required')
    with tempfile.TemporaryDirectory(prefix='weekend-native-consumers-') as temporary:
        directory = Path(temporary)
        env = compiler_environment(directory)
        search_path = next((value for key, value in env.items() if key.lower() == 'path'), '')
        compiler = shutil.which('cl.exe', path=search_path)
        if compiler is None:
            raise RuntimeError('Visual Studio environment did not provide cl.exe')
        shutil.copy2(library, directory / library.name)
        for name, language, standard in [('consumer-c', '/TC', '/std:c11'), ('consumer-cpp', '/TP', '/std:c++17')]:
            executable = directory / (name + '.exe')
            command = [compiler, '/nologo', '/W4', '/WX', language, standard,
                       '/I' + str(ROOT / 'include'), str(ROOT / 'examples/memory.c'),
                       '/Fo' + str(directory / (name + '.obj')), '/Fe' + str(executable), str(imports)]
            subprocess.run(command, env=env, cwd=directory, check=True)
            subprocess.run([str(executable)], env=env, cwd=directory, check=True)
    print('Compiled Windows C and C++ consumers passed against the delivered DLL')


if __name__ == '__main__':
    main()
