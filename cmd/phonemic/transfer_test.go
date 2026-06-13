package main

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

// TestPendingFileRoundTrip 验证文件暂存 → /download 取回字节的完整链路。
func TestPendingFileRoundTrip(t *testing.T) {
	ts := &transferState{pending: make(map[string]pendingFile)}
	want := []byte("hello phone download \x00\x01\xff 中文")
	id := ts.addPendingFile("测试.bin", want, "application/octet-stream")

	got, ok := ts.getPendingFile(id)
	if !ok {
		t.Fatalf("getPendingFile(%q) not found", id)
	}
	if string(got.data) != string(want) {
		t.Fatalf("data mismatch: got %q want %q", got.data, want)
	}
	if got.name != "测试.bin" {
		t.Fatalf("name mismatch: got %q", got.name)
	}
}

// TestPendingEviction 验证超过数量上限时淘汰最旧。
func TestPendingEviction(t *testing.T) {
	ts := &transferState{pending: make(map[string]pendingFile)}
	ids := make([]string, 0, pendingMaxCount+5)
	for i := 0; i < pendingMaxCount+5; i++ {
		ids = append(ids, ts.addPendingFile("f", []byte{byte(i)}, ""))
	}
	// 最早的 5 个应被淘汰
	for i := 0; i < 5; i++ {
		if _, ok := ts.getPendingFile(ids[i]); ok {
			t.Errorf("expected id[%d] evicted, still present", i)
		}
	}
	// 最新的应在
	if _, ok := ts.getPendingFile(ids[len(ids)-1]); !ok {
		t.Errorf("newest id should be present")
	}
}

// TestSentRingBuffer 验证发送记录环形缓冲：新的在前、超容覆盖最旧。
func TestSentRingBuffer(t *testing.T) {
	ts := &transferState{pending: make(map[string]pendingFile)}
	for i := 0; i < sentLogCap+3; i++ {
		ts.addSent("text", string(rune('A'+i%26)))
	}
	got := ts.recentSent()
	if len(got) != sentLogCap {
		t.Fatalf("recentSent len = %d, want %d", len(got), sentLogCap)
	}
}

// TestHandleDownloadAuth 验证 /download 鉴权与 404/200。
func TestHandleDownloadAuth(t *testing.T) {
	state := &appState{pairCode: "TESTCD"}
	// 预置一份文件
	id := transfer.addPendingFile("a.txt", []byte("xyz"), "text/plain")
	h := handleDownload(state)

	// 错配对码 → 403
	rec := httptest.NewRecorder()
	h(rec, httptest.NewRequest(http.MethodGet, "/download?id="+id+"&code=WRONG", nil))
	if rec.Code != http.StatusForbidden {
		t.Errorf("wrong code: got %d want 403", rec.Code)
	}

	// 不存在 id → 404
	rec = httptest.NewRecorder()
	h(rec, httptest.NewRequest(http.MethodGet, "/download?id=nope&code=TESTCD", nil))
	if rec.Code != http.StatusNotFound {
		t.Errorf("missing id: got %d want 404", rec.Code)
	}

	// 正确 → 200 + 字节
	rec = httptest.NewRecorder()
	h(rec, httptest.NewRequest(http.MethodGet, "/download?id="+id+"&code=TESTCD", nil))
	if rec.Code != http.StatusOK {
		t.Errorf("ok: got %d want 200", rec.Code)
	}
	if rec.Body.String() != "xyz" {
		t.Errorf("body: got %q want xyz", rec.Body.String())
	}
}
