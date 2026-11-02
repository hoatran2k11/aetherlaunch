@echo off
REM Mở "x86 Native Tools Command Prompt for VS 2022" (KHÔNG phải x64) rồi chạy file này.
REM Kết quả: src-tauri\resources\samp_helper.exe (32-bit)
cd /d "%~dp0"
if not exist build mkdir build
cl /nologo /O2 /MT /EHsc /DUNICODE /D_UNICODE samp_helper.cpp /Fobuild\ /Fe:..\src-tauri\resources\samp_helper.exe
if errorlevel 1 (echo BUILD FAILED & exit /b 1)
echo OK: src-tauri\resources\samp_helper.exe

REM --- Nếu dùng MinGW (i686) thay cho MSVC: ---
REM i686-w64-mingw32-g++ -O2 -municode -static -o ..\src-tauri\resources\samp_helper.exe samp_helper.cpp
