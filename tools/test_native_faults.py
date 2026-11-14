"""Compatibility entry point delegates fault orchestration to the private tool."""
import unittest
import subprocess
import sys
from pathlib import Path
from unittest.mock import patch
import native_faults
class NativeFaultRunnerAdmission(unittest.TestCase):
    def test_compatibility_cli_delegates_exact_arguments(self):
        with patch('sys.argv',['native_faults.py','--source','HEAD','--output','new']),patch.object(native_faults.supervision,'command') as command:
            native_faults.main()
        command.assert_called_once_with(['faults','native','--source','HEAD','--output','new'])
        result=subprocess.run([sys.executable,'-I','-B',str(Path(native_faults.__file__).resolve()),'--help'],capture_output=True,text=True,timeout=5)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertIn('Usage: xtask faults native',result.stdout)
if __name__=='__main__':unittest.main()
