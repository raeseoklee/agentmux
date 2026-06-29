# AgentMux 릴리즈 정책

AgentMux 릴리즈 문서의 기준 언어는 영어입니다. 원문 정책은
[docs/release-policy.md](../release-policy.md)에 있습니다.

## 릴리즈 주기

AgentMux는 작은 구현 커밋을 유지하되, 사용자에게 배포되는 릴리즈는
하나의 명확한 주제로 묶습니다. 예를 들면 터미널 복원 안정화, 릴리즈
자동화, 공개 문서 정리처럼 사용자가 이해할 수 있는 단위가 좋습니다.

일반 릴리즈는 최소 두 개 이상의 실질 작업 조각을 포함하고, 긴급하지
않은 경우 세 개에서 다섯 개 정도의 작업 조각을 묶는 것을 권장합니다.

## 패치, 마이너, 프리릴리즈 기준

- 패치 릴리즈는 안정 계약을 깨지 않는 추가 기능, 회귀 수정, 문서 개선,
  릴리즈 자동화 보강에 사용합니다.
- 마이너 릴리즈는 안정 계약 승격이나 호환성에 영향을 주는 변경에
  사용합니다.
- 프리릴리즈는 명시적으로 선택한 채널에서만 사용하며, 안정 채널을
  대체하지 않습니다.

## 핫픽스 예외

다음 상황에서는 작은 패치 릴리즈를 허용합니다.

- 보안 문제.
- 설치 또는 배포 실패.
- CI, 릴리즈, 업데이트, attestation 차단 문제.
- 현재 안정 릴리즈의 심각한 회귀.

## 증적 요구사항

릴리즈 태그를 푸시하기 전에 다음 로컬 검증을 실행합니다.

```powershell
npm run version:check
npm --prefix apps/desktop run build
npm run docs:check
npm run repo:hygiene
npm run check
```

태그 workflow가 릴리즈를 게시한 뒤에는 다음을 확인합니다.

- 예상 태그의 GitHub Release가 존재하는지.
- Windows 설치 파일, 체크섬, 업데이트 아티팩트, 업데이트 서명,
  `latest.json`이 업로드되었는지.
- 릴리즈 아티팩트의 GitHub Artifact Attestation이 검증되는지.
- SHA256 파일과 다운로드한 설치 파일의 해시가 일치하는지.

운영 명령은 [release-runbook.md](../en/operations/release-runbook.md)와
[versioning.md](../en/release/versioning.md)를 참고하세요.
