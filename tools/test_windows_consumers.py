"""Regression checks for the native MSVC consumer command."""
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

import windows_consumers


class WindowsConsumerCommands(unittest.TestCase):
    def test_import_library_is_passed_to_linker_in_both_languages(self):
        with tempfile.TemporaryDirectory() as temporary:
            library = Path(temporary).resolve() / 'weekend_db.dll'
            library.write_bytes(b'dll')
            imports = library.with_suffix('.dll.lib')
            imports.write_bytes(b'!<arch>\n')
            with (
                patch.object(windows_consumers.sys, 'platform', 'win32'),
                patch.object(windows_consumers.sys, 'argv', ['windows_consumers.py', '--library', str(library)]),
                patch.object(windows_consumers, 'compiler_environment', return_value={'PATH': 'compiler'}),
                patch.object(windows_consumers.shutil, 'which', return_value='cl.exe'),
                patch.object(windows_consumers.subprocess, 'run') as run,
            ):
                windows_consumers.main()
            commands = [call.args[0] for call in run.call_args_list]
            self.assertEqual(len(commands), 4)
            for command, language in [(commands[0], '/TC'), (commands[2], '/TP')]:
                self.assertIn(language, command)
                self.assertEqual(command[command.index('/link') + 1:], [str(imports)])
                self.assertNotIn(str(imports), command[:command.index('/link')])
                self.assertIn(str(windows_consumers.ROOT / 'examples/memory.c'), command[:command.index('/link')])
            self.assertTrue(commands[1][0].endswith('consumer-c.exe'))
            self.assertTrue(commands[3][0].endswith('consumer-cpp.exe'))


if __name__ == '__main__':
    unittest.main()
