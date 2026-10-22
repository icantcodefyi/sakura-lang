(* Matched port of programs/tokenizer. The text is built in a Buffer instead of repeated
   string concatenation; scalars are counted by decoding UTF-8 with String.get_utf_8_uchar. *)
let words = [| "alpha"; "naïve"; "日本語"; "🌸"; "x1"; "Grüße"; "ok"; "mañana" |]

let scalar_count t =
  let rec go i acc =
    if i >= String.length t then acc
    else go (i + Uchar.utf_decode_length (String.get_utf_8_uchar t i)) (acc + 1)
  in
  go 0 0

let tokenizer n =
  let b = Buffer.create (n * 8) in
  for i = 0 to n - 1 do
    Buffer.add_string b words.(i mod 8);
    Buffer.add_string b (if i mod 5 = 4 then ", " else " ")
  done;
  let tokens = ref 0 and bytes = ref 0 and scalars = ref 0 and commas = ref 0 in
  List.iter
    (fun t ->
      if t <> "" then begin
        incr tokens;
        bytes := !bytes + String.length t;
        scalars := !scalars + scalar_count t;
        if String.ends_with ~suffix:"," t then incr commas
      end)
    (String.split_on_char ' ' (Buffer.contents b));
  !tokens + (3 * !bytes) + (7 * !scalars) + (11 * !commas)

let () = Runner.main tokenizer 100000
