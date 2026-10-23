// The social app of bench/http/README.md on net/http and database/sql with
// modernc.org/sqlite (pure Go, in memory). `main <port>` serves on 127.0.0.1:<port>.
// AI-assisted (Claude).
//
// An in-memory SQLite database belongs to one connection, so the pool is held at one
// connection: every query is serialized on it, as in the single-threaded servers.
package main

import (
	"database/sql"
	"encoding/json"
	"errors"
	"io"
	"log"
	"net/http"
	"os"
	"regexp"
	"strconv"
	"unicode/utf8"

	_ "modernc.org/sqlite"
)

const maxBody = 4096

type user struct {
	ID   int64  `json:"id"`
	Name string `json:"name"`
}

type post struct {
	ID     int64  `json:"id"`
	UserID int64  `json:"user_id"`
	Body   string `json:"body"`
	Likes  int64  `json:"likes"`
}

type likes struct {
	PostID int64 `json:"post_id"`
	Likes  int64 `json:"likes"`
}

type app struct {
	insertUser, insertPost, selectPost, insertLike, countLikes, selectFeed *sql.Stmt
}

func must(s *sql.Stmt, err error) *sql.Stmt {
	if err != nil {
		log.Fatal(err)
	}
	return s
}

func open() *app {
	db, err := sql.Open("sqlite", ":memory:")
	if err != nil {
		log.Fatal(err)
	}
	db.SetMaxOpenConns(1)
	db.SetMaxIdleConns(1)
	db.SetConnMaxLifetime(0)
	for _, s := range []string{
		"create table users (id integer primary key, name text not null)",
		"create table posts (id integer primary key, user_id integer not null, body text not null)",
		"create table likes (post_id integer not null, user_id integer not null, primary key (post_id, user_id))",
	} {
		if _, err := db.Exec(s); err != nil {
			log.Fatal(err)
		}
	}
	return &app{
		insertUser: must(db.Prepare("insert into users (name) values (?) returning id, name")),
		insertPost: must(db.Prepare("insert into posts (user_id, body) select id, ? from users where id = ? returning id, user_id, body, 0 as likes")),
		selectPost: must(db.Prepare("select id, user_id, body, (select count(*) from likes where post_id = posts.id) as likes from posts where id = ?")),
		insertLike: must(db.Prepare("insert or ignore into likes (post_id, user_id) select posts.id, users.id from posts, users where posts.id = ? and users.id = ?")),
		countLikes: must(db.Prepare("select id as post_id, (select count(*) from likes where post_id = posts.id) as likes from posts where id = ?")),
		selectFeed: must(db.Prepare("select id, user_id, body, (select count(*) from likes where post_id = posts.id) as likes from posts order by id desc limit ?")),
	}
}

func text(w http.ResponseWriter, status int, body string) {
	w.Header().Set("content-type", "text/plain")
	w.WriteHeader(status)
	io.WriteString(w, body)
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	b, err := json.Marshal(v)
	if err != nil {
		text(w, 500, err.Error())
		return
	}
	w.Header().Set("content-type", "application/json")
	w.WriteHeader(status)
	w.Write(b)
}

// decode reads the body into v, or answers 400 and returns false.
func decode(w http.ResponseWriter, r *http.Request, v any) bool {
	b, err := io.ReadAll(io.LimitReader(r.Body, maxBody+1))
	if err != nil || len(b) > maxBody {
		text(w, 400, "body too large")
		return false
	}
	if err := json.Unmarshal(b, v); err != nil {
		text(w, 400, "malformed JSON")
		return false
	}
	return true
}

func isText(s string, max int) bool {
	n := utf8.RuneCountInString(s)
	return n >= 1 && n <= max
}

var digits = regexp.MustCompile(`^\d+$`)

func postID(r *http.Request) int64 {
	s := r.PathValue("id")
	if !digits.MatchString(s) {
		return 0
	}
	id, _ := strconv.ParseInt(s, 10, 64)
	return id
}

func (a *app) createUser(w http.ResponseWriter, r *http.Request) {
	var in struct {
		Name *string `json:"name"`
	}
	if !decode(w, r, &in) {
		return
	}
	if in.Name == nil || !isText(*in.Name, 64) {
		text(w, 400, "name: 1 to 64 characters")
		return
	}
	var u user
	if err := a.insertUser.QueryRow(*in.Name).Scan(&u.ID, &u.Name); err != nil {
		text(w, 500, err.Error())
		return
	}
	writeJSON(w, 201, u)
}

func (a *app) createPost(w http.ResponseWriter, r *http.Request) {
	var in struct {
		UserID *int64  `json:"user_id"`
		Body   *string `json:"body"`
	}
	if !decode(w, r, &in) {
		return
	}
	if in.UserID == nil || in.Body == nil || !isText(*in.Body, 280) {
		text(w, 400, "user_id and body: 1 to 280 characters")
		return
	}
	var p post
	err := a.insertPost.QueryRow(*in.Body, *in.UserID).Scan(&p.ID, &p.UserID, &p.Body, &p.Likes)
	switch {
	case errors.Is(err, sql.ErrNoRows):
		text(w, 404, "no such user")
	case err != nil:
		text(w, 500, err.Error())
	default:
		writeJSON(w, 201, p)
	}
}

func (a *app) getPost(w http.ResponseWriter, r *http.Request) {
	var p post
	err := a.selectPost.QueryRow(postID(r)).Scan(&p.ID, &p.UserID, &p.Body, &p.Likes)
	switch {
	case errors.Is(err, sql.ErrNoRows):
		text(w, 404, "no such post")
	case err != nil:
		text(w, 500, err.Error())
	default:
		writeJSON(w, 200, p)
	}
}

func (a *app) likePost(w http.ResponseWriter, r *http.Request) {
	id := postID(r)
	if id == 0 {
		text(w, 404, "no such post")
		return
	}
	var in struct {
		UserID *int64 `json:"user_id"`
	}
	if !decode(w, r, &in) {
		return
	}
	if in.UserID == nil {
		text(w, 400, "user_id")
		return
	}
	if _, err := a.insertLike.Exec(id, *in.UserID); err != nil {
		text(w, 500, err.Error())
		return
	}
	var l likes
	err := a.countLikes.QueryRow(id).Scan(&l.PostID, &l.Likes)
	switch {
	case errors.Is(err, sql.ErrNoRows):
		text(w, 404, "no such post")
	case err != nil:
		text(w, 500, err.Error())
	default:
		writeJSON(w, 200, l)
	}
}

func (a *app) feed(w http.ResponseWriter, r *http.Request) {
	n, err := strconv.Atoi(r.URL.Query().Get("limit"))
	if err != nil {
		n = 20
	}
	n = min(max(n, 1), 100)
	rows, err := a.selectFeed.Query(n)
	if err != nil {
		text(w, 500, err.Error())
		return
	}
	defer rows.Close()
	posts := []post{}
	for rows.Next() {
		var p post
		if err := rows.Scan(&p.ID, &p.UserID, &p.Body, &p.Likes); err != nil {
			text(w, 500, err.Error())
			return
		}
		posts = append(posts, p)
	}
	writeJSON(w, 200, struct {
		Posts []post `json:"posts"`
	}{posts})
}

func main() {
	port := "8080"
	if len(os.Args) > 1 {
		port = os.Args[1]
	}
	a := open()
	mux := http.NewServeMux()
	mux.HandleFunc("GET /health", func(w http.ResponseWriter, _ *http.Request) { text(w, 200, "ok") })
	mux.HandleFunc("POST /users", a.createUser)
	mux.HandleFunc("POST /posts", a.createPost)
	mux.HandleFunc("GET /posts/{id}", a.getPost)
	mux.HandleFunc("POST /posts/{id}/like", a.likePost)
	mux.HandleFunc("GET /feed", a.feed)
	log.Fatal(http.ListenAndServe("127.0.0.1:"+port, mux))
}
