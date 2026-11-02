// samp_helper.exe  (BẮT BUỘC build x86 / 32-bit)
// Dựa trên ý tưởng của sampcmd: tạo gta_sa.exe ở trạng thái SUSPENDED,
// chèn samp.dll bằng LoadLibraryW qua remote thread, chờ nạp xong rồi mới resume.
//
// Khác sampcmd:
//   - Nhận thư mục game qua tham số (không phụ thuộc thư mục hiện tại)
//   - Đặt working directory của gta_sa.exe = thư mục game
//   - Kiểm tra kết quả LoadLibrary; nếu lỗi thì kill tiến trình thay vì để game chạy hỏng
//   - Báo lỗi qua stderr dạng "ERR|bước|mã" + exit code, không bật MessageBox
//
// Dùng:  samp_helper.exe <thư_mục_game> [tham số truyền cho gta_sa.exe...]
// VD:    samp_helper.exe "D:\GTA San Andreas" -c -h 127.0.0.1 -p 7777 -n Max_Bennett

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cstdio>
#include <string>
#include <vector>

using namespace std;

static int fail(const char* step, DWORD code)
{
	fprintf(stderr, "ERR|%s|%lu\n", step, static_cast<unsigned long>(code));
	return code ? static_cast<int>(code) : 1;
}

static wstring quote(const wstring& a)
{
	if (!a.empty() && a.find_first_of(L" \t\"") == wstring::npos)
		return a;
	wstring r = L"\"";
	for (wchar_t c : a)
	{
		if (c == L'"') r += L'\\';
		r += c;
	}
	r += L'"';
	return r;
}

int wmain(int argc, wchar_t** argv)
{
	if (argc < 2)
	{
		fprintf(stderr, "Usage: samp_helper.exe <game_dir> [args...]\n");
		return 2;
	}

	wstring dir = argv[1];
	while (dir.size() > 3 && (dir.back() == L'\\' || dir.back() == L'/'))
		dir.pop_back();

	const wstring exe = dir + L"\\gta_sa.exe";
	const wstring dll = dir + L"\\samp.dll";

	if (GetFileAttributesW(exe.c_str()) == INVALID_FILE_ATTRIBUTES) return fail("gta_sa.exe", GetLastError());
	if (GetFileAttributesW(dll.c_str()) == INVALID_FILE_ATTRIBUTES) return fail("samp.dll", GetLastError());

	wstring cmd = quote(exe);
	for (int i = 2; i < argc; i++)
	{
		cmd += L' ';
		cmd += quote(argv[i]);
	}

	HMODULE k32 = GetModuleHandleW(L"kernel32.dll");
	FARPROC load = k32 ? GetProcAddress(k32, "LoadLibraryW") : nullptr;
	if (!load) return fail("LoadLibraryW", GetLastError());

	STARTUPINFOW si = {};
	si.cb = sizeof(si);
	PROCESS_INFORMATION pi = {};
	vector<wchar_t> buf(cmd.begin(), cmd.end());
	buf.push_back(0);

	if (!CreateProcessW(exe.c_str(), buf.data(), nullptr, nullptr, FALSE,
		DETACHED_PROCESS | CREATE_SUSPENDED, nullptr, dir.c_str(), &si, &pi))
		return fail("CreateProcess", GetLastError());

	const char* step = nullptr;
	DWORD err = 0;
	const SIZE_T bytes = (dll.size() + 1) * sizeof(wchar_t);

	LPVOID remote = VirtualAllocEx(pi.hProcess, nullptr, bytes, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
	if (!remote)
	{
		step = "VirtualAllocEx";
		err = GetLastError();
	}
	else
	{
		if (!WriteProcessMemory(pi.hProcess, remote, dll.c_str(), bytes, nullptr))
		{
			step = "WriteProcessMemory";
			err = GetLastError();
		}
		else
		{
			HANDLE rt = CreateRemoteThread(pi.hProcess, nullptr, 0,
				reinterpret_cast<LPTHREAD_START_ROUTINE>(load), remote, 0, nullptr);
			if (!rt)
			{
				step = "CreateRemoteThread";
				err = GetLastError();
			}
			else
			{
				if (WaitForSingleObject(rt, 15000) != WAIT_OBJECT_0)
				{
					step = "WaitLoadLibrary";
					err = WAIT_TIMEOUT;
				}
				else
				{
					DWORD module = 0; // HMODULE (32-bit) trả về từ LoadLibraryW
					GetExitCodeThread(rt, &module);
					if (module == 0)
					{
						step = "LoadLibrary(samp.dll)";
						err = ERROR_MOD_NOT_FOUND; // 126
					}
				}
				CloseHandle(rt);
			}
		}
		VirtualFreeEx(pi.hProcess, remote, 0, MEM_RELEASE);
	}

	if (step)
	{
		TerminateProcess(pi.hProcess, 1);
		CloseHandle(pi.hThread);
		CloseHandle(pi.hProcess);
		return fail(step, err);
	}

	ResumeThread(pi.hThread);
	CloseHandle(pi.hThread);
	CloseHandle(pi.hProcess);
	printf("OK\n");
	return 0;
}
