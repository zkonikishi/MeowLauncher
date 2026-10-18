import json,os,subprocess,unittest
from unittest.mock import patch
from windows_inspection import inspection_environment
from supervised import powershell

class InspectionTests(unittest.TestCase):
 def test_removes_parent_module_path_case_insensitively_without_mutation(self):
  source={"PSModulePath":"pwsh7-modules","pSmOdUlEpAtH":"also-polluted","MEOW_REVIEW_PID":"123","PATH":"kept"}
  self.assertEqual(inspection_environment(source),{"MEOW_REVIEW_PID":"123","PATH":"kept"})
  self.assertIn("PSModulePath",source)
 def test_supervised_default_and_explicit_environment_are_isolated(self):
  with patch.dict(os.environ,{"PSModulePath":"polluted"}),patch("supervised.subprocess.run",return_value=subprocess.CompletedProcess([],0,b'{"ok":true}',b'')) as run:
   self.assertEqual(powershell("fixture"),{"ok":True})
   self.assertFalse(any(k.casefold()=="psmodulepath" for k in run.call_args.kwargs["env"]))
   powershell("fixture",{"PSModulePath":"polluted","MEOW_REVIEW_PID":"9"})
   self.assertEqual(run.call_args.kwargs["env"],{"MEOW_REVIEW_PID":"9"})
 @unittest.skipUnless(os.name=="nt","Windows PowerShell only")
 def test_actual_windows_powershell_can_load_security_and_cim_from_polluted_parent(self):
  source=dict(os.environ);source["PSModulePath"]="C:/dev-toolchain/tmp/nonexistent-pwsh7-modules"
  script="$ErrorActionPreference='Stop'; Import-Module Microsoft.PowerShell.Security; Import-Module CimCmdlets; @{security=[bool](Get-Command Get-Acl);cim=[bool](Get-Command Get-CimInstance)}|ConvertTo-Json -Compress"
  with patch.dict(os.environ,source,clear=True):
   result=powershell(script)
  self.assertEqual(result,{"security":True,"cim":True})
if __name__=="__main__":unittest.main(verbosity=2)
