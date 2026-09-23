.PHONY: bootstrap bootstrap-full check check-package test test-mcp test-mcp-launcher test-full start

bootstrap:
	powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\agent-bootstrap.ps1 -SkipWorkspaceCheck

bootstrap-full:
	powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\agent-bootstrap.ps1

check:
	powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify.ps1 -Lane Fast

check-package:
	powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify.ps1 -Lane Package -Package "$(PACKAGE)"

test:
	powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify.ps1 -Lane NonUi

test-mcp:
	powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify.ps1 -Lane Mcp

test-mcp-launcher:
	powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify.ps1 -Lane McpLauncher

test-full:
	powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify.ps1 -Lane Workspace

start:
	watchexec -e rs -r cargo run
