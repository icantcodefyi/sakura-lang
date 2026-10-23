package main

// A minimal HTTP/1.1 client over one keep-alive connection: a request line, Host, an optional
// JSON body, and an answer read to its end by content-length, chunked coding or close.
// net/http's client spends more CPU per request than the fastest servers here spend
// answering; this one leaves the load generator's cores free (see the README's calibration).

import (
	"bufio"
	"bytes"
	"errors"
	"fmt"
	"io"
	"net"
	"strconv"
	"time"
)

type conn struct {
	addr, host string
	timeout    time.Duration
	c          net.Conn
	r          *bufio.Reader
	w          *bufio.Writer
}

func (k *conn) close() {
	if k.c != nil {
		k.c.Close()
		k.c = nil
	}
}

var errMalformed = errors.New("malformed answer")

// dialError marks a failure to open the connection, as opposed to one on an open connection.
type dialError struct{ err error }

func (e dialError) Error() string { return e.err.Error() }
func (e dialError) Unwrap() error { return e.err }

// do sends q and reads the answer; on any error the connection is closed and the next call
// dials again.
func (k *conn) do(q request) (int, error) {
	status, err := k.roundTrip(q)
	if err != nil {
		k.close()
	}
	return status, err
}

func (k *conn) dial() error {
	c, err := net.DialTimeout("tcp", k.addr, k.timeout)
	if err != nil {
		return dialError{err}
	}
	k.c, k.r, k.w = c, bufio.NewReaderSize(c, 16<<10), bufio.NewWriterSize(c, 4<<10)
	return nil
}

func (k *conn) roundTrip(q request) (int, error) {
	if k.c == nil {
		if err := k.dial(); err != nil {
			return 0, err
		}
	}
	k.c.SetDeadline(time.Now().Add(k.timeout))
	fmt.Fprintf(k.w, "%s %s HTTP/1.1\r\nHost: %s\r\n", q.method, q.path, k.host)
	if q.body != nil {
		fmt.Fprintf(k.w, "Content-Type: application/json\r\nContent-Length: %d\r\n", len(q.body))
	}
	k.w.WriteString("\r\n")
	k.w.Write(q.body)
	if err := k.w.Flush(); err != nil {
		return 0, err
	}
	line, err := k.line()
	if err != nil {
		return 0, err
	}
	// "HTTP/1.1 200 OK"
	if len(line) < 12 || !bytes.HasPrefix(line, []byte("HTTP/1.")) {
		return 0, errMalformed
	}
	status, err := strconv.Atoi(string(line[9:12]))
	if err != nil {
		return 0, errMalformed
	}
	length, chunked, closing := -1, false, line[7] == '0'
	for {
		h, err := k.line()
		if err != nil {
			return 0, err
		}
		if len(h) == 0 {
			break
		}
		colon := bytes.IndexByte(h, ':')
		if colon < 0 {
			return 0, errMalformed
		}
		name, value := bytes.ToLower(h[:colon]), bytes.ToLower(bytes.TrimSpace(h[colon+1:]))
		switch string(name) {
		case "content-length":
			if length, err = strconv.Atoi(string(value)); err != nil || length < 0 {
				return 0, errMalformed
			}
		case "transfer-encoding":
			chunked = bytes.Contains(value, []byte("chunked"))
		case "connection":
			closing = bytes.Contains(value, []byte("close"))
		}
	}
	switch {
	case q.method == "HEAD" || status == 204 || status == 304:
	case chunked:
		if err := k.chunks(); err != nil {
			return 0, err
		}
	case length >= 0:
		if _, err := k.r.Discard(length); err != nil {
			return 0, err
		}
	default:
		if _, err := io.Copy(io.Discard, k.r); err != nil {
			return 0, err
		}
		closing = true
	}
	if closing {
		k.close()
	}
	return status, nil
}

// line reads one CRLF-terminated line without its terminator.
func (k *conn) line() ([]byte, error) {
	l, err := k.r.ReadSlice('\n')
	if err != nil {
		if err == bufio.ErrBufferFull {
			return nil, errMalformed
		}
		return nil, err
	}
	return bytes.TrimRight(l, "\r\n"), nil
}

func (k *conn) chunks() error {
	for {
		l, err := k.line()
		if err != nil {
			return err
		}
		if i := bytes.IndexByte(l, ';'); i >= 0 {
			l = l[:i]
		}
		n, err := strconv.ParseInt(string(bytes.TrimSpace(l)), 16, 64)
		if err != nil || n < 0 {
			return errMalformed
		}
		if n == 0 {
			for { // trailers, then the empty line
				t, err := k.line()
				if err != nil {
					return err
				}
				if len(t) == 0 {
					return nil
				}
			}
		}
		if _, err := k.r.Discard(int(n) + 2); err != nil {
			return err
		}
	}
}
