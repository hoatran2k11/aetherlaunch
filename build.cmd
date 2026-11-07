@echo off
chcp 65001 > nul
:menu
cls
echo ====================================================
echo                 MENU TAURI V2
echo ====================================================
echo 1. Chay thu (npm run tauri dev)
echo 2. Dong goi ban cai dat (npm run tauri build)
echo 3. Thoat
echo ====================================================
set /p choice=Lua chon cua ban (1-3): 

if "%choice%"=="1" goto dev
if "%choice%"=="2" goto build
if "%choice%"=="3" exit

echo Lua chon khong hop le, vui long chon lai!
pause
goto menu

:dev
cls
echo Dang khoi dong che do chay thu (Dev)...
echo (Luu y: Lan dau Cargo se tai va bien dich nhieu crate nen mat vai phut)
npm run tauri dev
pause
goto menu

:build
cls
echo Dang tien hanh dong goi ban cai dat (Build)...
echo (File cai dat se nam trong src-tauri/target/release/bundle/)
npm run tauri build
pause
goto menu