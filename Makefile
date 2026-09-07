.PHONY: bootstrap bootstrap-full check test test-mcp test-full start

bootstrap:
	powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\agent-bootstrap.ps1 -SkipWorkspaceCheck

bootstrap-full:
	powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\agent-bootstrap.ps1

check:
	powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify.ps1 -Lane Fast

test:
	powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify.ps1 -Lane NonUi

test-mcp:
	powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify.ps1 -Lane Mcp

test-full:
	powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify.ps1 -Lane Workspace

start:
	watchexec -e rs -r cargo run
