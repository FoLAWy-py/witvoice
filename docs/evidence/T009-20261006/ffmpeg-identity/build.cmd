@echo off
call "D:\Software\WitvoiceToolchain\VS2022BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b %errorlevel%
cl /nologo /W4 /WX /EHsc /std:c++17 /utf-8 "D:\Project\witvoice\.local\ffmpeg-identity\identity.cpp" /Fo"D:\Project\witvoice\.local\ffmpeg-identity\identity.obj" /Fe"D:\Project\witvoice\.local\ffmpeg-identity\identity.exe" /link ole32.lib oleaut32.lib strmiids.lib winmm.lib
exit /b %errorlevel%
