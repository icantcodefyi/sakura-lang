(* Matched port of programs/json_validate with a stdlib-only recursive-descent JSON parser
   (RFC 8259; duplicate keys keep the last value; lone surrogate escapes are rejected, as
   serde_json does) in place of serde_json, and a hand-written string encoder. *)
type json =
  | Null
  | Bool of bool
  | Number of float
  | String of string
  | Array of json list
  | Object of (string * json) list (* latest key first *)

exception Invalid

let parse (s : string) : json =
  let len = String.length s in
  let pos = ref 0 in
  let peek () = if !pos < len then s.[!pos] else '\000' in
  let expect c = if peek () = c && !pos < len then incr pos else raise Invalid in
  let rec skip_ws () =
    match peek () with
    | (' ' | '\t' | '\n' | '\r') when !pos < len -> incr pos; skip_ws ()
    | _ -> ()
  in
  let literal word v =
    let l = String.length word in
    if !pos + l <= len && String.sub s !pos l = word then (pos := !pos + l; v) else raise Invalid
  in
  let hex4 () =
    if !pos + 4 > len then raise Invalid;
    let v = ref 0 in
    for k = 0 to 3 do
      let d =
        match s.[!pos + k] with
        | '0' .. '9' as c -> Char.code c - 48
        | 'a' .. 'f' as c -> Char.code c - 87
        | 'A' .. 'F' as c -> Char.code c - 55
        | _ -> raise Invalid
      in
      v := (!v * 16) + d
    done;
    pos := !pos + 4;
    !v
  in
  let string_ () =
    expect '"';
    let b = Buffer.create 16 in
    let rec go () =
      if !pos >= len then raise Invalid;
      let c = s.[!pos] in
      incr pos;
      match c with
      | '"' -> Buffer.contents b
      | '\\' ->
        if !pos >= len then raise Invalid;
        let e = s.[!pos] in
        incr pos;
        (match e with
         | '"' | '\\' | '/' -> Buffer.add_char b e
         | 'b' -> Buffer.add_char b '\b'
         | 'f' -> Buffer.add_char b '\012'
         | 'n' -> Buffer.add_char b '\n'
         | 'r' -> Buffer.add_char b '\r'
         | 't' -> Buffer.add_char b '\t'
         | 'u' ->
           let u = hex4 () in
           let code =
             if u >= 0xD800 && u <= 0xDBFF then begin
               expect '\\';
               expect 'u';
               let lo = hex4 () in
               if lo < 0xDC00 || lo > 0xDFFF then raise Invalid;
               0x10000 + ((u - 0xD800) lsl 10) + (lo - 0xDC00)
             end
             else if u >= 0xDC00 && u <= 0xDFFF then raise Invalid
             else u
           in
           Buffer.add_utf_8_uchar b (Uchar.of_int code)
         | _ -> raise Invalid);
        go ()
      | c when Char.code c < 0x20 -> raise Invalid
      | c -> Buffer.add_char b c; go ()
    in
    go ()
  in
  let number () =
    let start = !pos in
    let digits () =
      let d = !pos in
      while !pos < len && peek () >= '0' && peek () <= '9' do incr pos done;
      if !pos = d then raise Invalid
    in
    if peek () = '-' then incr pos;
    if peek () = '0' && !pos < len then incr pos else digits ();
    if peek () = '.' && !pos < len then (incr pos; digits ());
    if (peek () = 'e' || peek () = 'E') && !pos < len then begin
      incr pos;
      if peek () = '+' || peek () = '-' then incr pos;
      digits ()
    end;
    Number (float_of_string (String.sub s start (!pos - start)))
  in
  let rec value () =
    skip_ws ();
    if !pos >= len then raise Invalid;
    match peek () with
    | '{' ->
      incr pos;
      skip_ws ();
      if peek () = '}' then (incr pos; Object [])
      else
        let rec members acc =
          skip_ws ();
          let k = string_ () in
          skip_ws ();
          expect ':';
          let acc = (k, value ()) :: acc in
          skip_ws ();
          match peek () with
          | ',' -> incr pos; members acc
          | '}' -> incr pos; Object acc
          | _ -> raise Invalid
        in
        members []
    | '[' ->
      incr pos;
      skip_ws ();
      if peek () = ']' then (incr pos; Array [])
      else
        let rec items acc =
          let acc = value () :: acc in
          skip_ws ();
          match peek () with
          | ',' -> incr pos; items acc
          | ']' -> incr pos; Array (List.rev acc)
          | _ -> raise Invalid
        in
        items []
    | '"' -> String (string_ ())
    | 't' -> literal "true" (Bool true)
    | 'f' -> literal "false" (Bool false)
    | 'n' -> literal "null" Null
    | '-' | '0' .. '9' -> number ()
    | _ -> raise Invalid
  in
  let v = value () in
  skip_ws ();
  if !pos <> len then raise Invalid;
  v

let max_safe = 9_007_199_254_740_991.

(* The refinements of User: id is a safe integer, 1 <= bytes(name) <= 32, 0 <= age <= 150. *)
let decode line =
  match parse line with
  | Object fields -> (
    match List.assoc_opt "id" fields, List.assoc_opt "name" fields, List.assoc_opt "age" fields with
    | Some (Number id), Some (String name), Some (Number age)
      when Float.is_integer id && Float.abs id <= max_safe
           && String.length name > 0 && String.length name <= 32
           && Float.is_integer age && age >= 0. && age <= 150. ->
      Some (int_of_float id, name, int_of_float age)
    | _ -> None)
  | _ -> None
  | exception Invalid -> None

let add_json_string b s =
  Buffer.add_char b '"';
  String.iter
    (function
      | '"' -> Buffer.add_string b "\\\""
      | '\\' -> Buffer.add_string b "\\\\"
      | '\b' -> Buffer.add_string b "\\b"
      | '\012' -> Buffer.add_string b "\\f"
      | '\n' -> Buffer.add_string b "\\n"
      | '\r' -> Buffer.add_string b "\\r"
      | '\t' -> Buffer.add_string b "\\t"
      | c when Char.code c < 0x20 -> Printf.bprintf b "\\u%04x" (Char.code c)
      | c -> Buffer.add_char b c)
    s;
  Buffer.add_char b '"'

let encode (id, name, age) =
  let b = Buffer.create 48 in
  Printf.bprintf b "{\"id\":%d,\"name\":" id;
  add_json_string b name;
  Printf.bprintf b ",\"age\":%d}" age;
  Buffer.contents b

let line i =
  let name = if i mod 17 = 0 then "" else "user" ^ string_of_int i in
  let age = if i mod 13 = 0 then 200 else i mod 90 in
  Printf.sprintf "{\"id\": %d, \"name\": \"%s\", \"age\": %d}" i name age

let json_validate n =
  let text = String.concat "\n" (List.init n line) in
  let valid = ref 0 and bytes = ref 0 and ids = ref 0 and invalid = ref 0 in
  List.iter
    (fun l ->
      if l <> "" then
        match decode l with
        | Some ((id, _, _) as u) ->
          incr valid;
          bytes := !bytes + String.length (encode u);
          ids := !ids + id
        | None -> incr invalid)
    (String.split_on_char '\n' text);
  !valid + !bytes + !ids + (1000 * !invalid)

let () = Runner.main json_validate 30000
