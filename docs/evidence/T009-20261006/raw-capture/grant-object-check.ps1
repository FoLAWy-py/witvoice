$ErrorActionPreference='Stop'
if($PSVersionTable.PSVersion.Major -ne 5){throw 'SystemPS5 required'}
$path='D:\Project\witvoice\.local\t009-raw-once\once.json'
$raw=[IO.File]::ReadAllText($path)
$g=$raw|ConvertFrom-Json
if($g.status -ne 'AUTHORIZED_NOT_EXECUTED' -or -not($g.PSObject.Properties.Name -contains 'consumed_utc')){throw 'Missing unconsumed grant field'}
$g.status='CONSUMED_BEFORE_LAUNCH';$g.consumed_utc='2000-01-01T00:00:00Z'
$copy=$g|ConvertTo-Json -Depth 10|ConvertFrom-Json
if($copy.status -ne 'CONSUMED_BEFORE_LAUNCH' -or $copy.consumed_utc -ne '2000-01-01T00:00:00Z'){throw 'Grant assignment/roundtrip failed'}
if([IO.File]::ReadAllText($path) -ne $raw){throw 'Grant changed by pure check'}
Write-Output 'PASS_OBJECT_ASSIGNMENT_AND_ROUNDTRIP_ONLY; actual once unconsumed; no Process.Start/audio/model'
Write-Output $raw
