package main

import (
	"fmt"
	"sync"
	"testing"
	"time"

	"github.com/gorilla/websocket"
)

// 文字记录环形缓冲：未满时按追加顺序，recentTexts 新的在前
func TestAddText_OrderNewestFirst(t *testing.T) {
	s := &appState{}
	base := time.Unix(1000, 0)
	s.addText(base, "first")
	s.addText(base.Add(time.Second), "second")
	s.addText(base.Add(2*time.Second), "third")

	got := s.recentTexts()
	if len(got) != 3 {
		t.Fatalf("len = %d, want 3", len(got))
	}
	want := []string{"third", "second", "first"}
	for i, w := range want {
		if got[i].text != w {
			t.Errorf("got[%d].text = %q, want %q", i, got[i].text, w)
		}
	}
}

// 容量上限：满则丢最旧，只保留最近 textLogCap 条
func TestAddText_CapDropsOldest(t *testing.T) {
	s := &appState{}
	base := time.Unix(0, 0)
	total := textLogCap + 10
	for i := 0; i < total; i++ {
		s.addText(base.Add(time.Duration(i)*time.Second), fmt.Sprintf("msg-%d", i))
	}

	got := s.recentTexts()
	if len(got) != textLogCap {
		t.Fatalf("len = %d, want %d", len(got), textLogCap)
	}
	// 最新的应是 msg-(total-1)
	if want := fmt.Sprintf("msg-%d", total-1); got[0].text != want {
		t.Errorf("newest = %q, want %q", got[0].text, want)
	}
	// 最旧保留的应是 msg-(total-cap)
	if want := fmt.Sprintf("msg-%d", total-textLogCap); got[len(got)-1].text != want {
		t.Errorf("oldest = %q, want %q", got[len(got)-1].text, want)
	}
}

// recentTexts 返回的是快照副本，不应受后续写入影响
func TestRecentTexts_SnapshotIsCopy(t *testing.T) {
	s := &appState{}
	s.addText(time.Unix(1, 0), "a")
	snap := s.recentTexts()
	s.addText(time.Unix(2, 0), "b")
	if len(snap) != 1 || snap[0].text != "a" {
		t.Fatalf("snapshot mutated: %+v", snap)
	}
}

// 并发读写：用 -race 跑应无数据竞争
func TestAddText_ConcurrentRace(t *testing.T) {
	s := &appState{}
	var wg sync.WaitGroup
	for w := 0; w < 8; w++ {
		wg.Add(1)
		go func(w int) {
			defer wg.Done()
			for i := 0; i < 200; i++ {
				s.addText(time.Unix(int64(i), 0), fmt.Sprintf("w%d-%d", w, i))
			}
		}(w)
	}
	for r := 0; r < 4; r++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for i := 0; i < 200; i++ {
				_ = s.recentTexts()
			}
		}()
	}
	wg.Wait()
	if got := len(s.recentTexts()); got != textLogCap {
		t.Errorf("final len = %d, want %d", got, textLogCap)
	}
}

// connCount：register / unregister 后计数正确
func TestConnCount_RegisterUnregister(t *testing.T) {
	s := &appState{}
	if got := s.connCount(); got != 0 {
		t.Fatalf("initial count = %d, want 0", got)
	}
	c1 := &websocket.Conn{}
	c2 := &websocket.Conn{}
	un1 := s.registerConn(c1)
	un2 := s.registerConn(c2)
	if got := s.connCount(); got != 2 {
		t.Fatalf("after 2 registers count = %d, want 2", got)
	}
	un1()
	if got := s.connCount(); got != 1 {
		t.Fatalf("after 1 unregister count = %d, want 1", got)
	}
	un2()
	if got := s.connCount(); got != 0 {
		t.Errorf("after 2 unregisters count = %d, want 0", got)
	}
}
