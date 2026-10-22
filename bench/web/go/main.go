// TechEmpower-style JSON and plaintext server on net/http (bench/web/README.md).
// AI-assisted (Claude), reviewed by the contributor.
//
// `main <port>` (or PORT) serves on 127.0.0.1 until the process stops. The harness runs it
// with GOMAXPROCS=1 for the single-threaded lane and with Go's default (one per CPU) for the
// multi-threaded lane. `/json` marshals a fresh struct per request; net/http adds `Date`.
package main

import (
	"encoding/json"
	"net/http"
	"os"
	"strconv"
)

type message struct {
	Message string `json:"message"`
}

func main() {
	port := "8080"
	if len(os.Args) > 1 {
		port = os.Args[1]
	} else if p := os.Getenv("PORT"); p != "" {
		port = p
	}
	mux := http.NewServeMux()
	mux.HandleFunc("GET /json", func(w http.ResponseWriter, _ *http.Request) {
		body, err := json.Marshal(message{Message: "Hello, World!"})
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		h := w.Header()
		h.Set("Content-Type", "application/json")
		h.Set("Content-Length", strconv.Itoa(len(body)))
		h.Set("Server", "go")
		w.Write(body)
	})
	mux.HandleFunc("GET /plaintext", func(w http.ResponseWriter, _ *http.Request) {
		h := w.Header()
		h.Set("Content-Type", "text/plain")
		h.Set("Content-Length", "13")
		h.Set("Server", "go")
		w.Write([]byte("Hello, World!"))
	})
	if err := http.ListenAndServe("127.0.0.1:"+port, mux); err != nil {
		os.Stderr.WriteString(err.Error() + "\n")
		os.Exit(1)
	}
}
