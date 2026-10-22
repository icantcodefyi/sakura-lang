// Json from Are We Fast Yet (smarr/are-we-fast-yet@74306fec, benchmarks/JavaScript/json.js),
// derived from the SOM benchmark, itself based on the Java version.
// Copyright (c) 2015-2016 Stefan Marr <git@stefan-marr.de>; MIT licence (text in ../js/main.mjs).
// AWFY's JsonValue classes are a variant (the literals' three singletons are its cases). The
// parser is a value every parser function takes and returns, as in the Sakura port, and AWFY's
// loops are tail-recursive functions, so the parser and the vectors being filled stay uniquely
// owned and Perceus updates them in place (a ref per field, or `var`s inside the loops, ran
// markedly slower in a rough check). som's Vector is a growable vector (initial size 10,
// doubled when full). `current` is the byte at `index` of `input.bytes` (AWFY's one-character
// substring), -1 for its null; a capture is `bytes.substring`. Koka's `int` has no bitwise
// operators, so the table's `& (length - 1)` and `& 0xff` are `%` (as in Sakura), equal on the
// nonnegative values they see. A parse error throws AWFY's message. run(n) adds the
// `operations` array's size (156) over n parses, 0 where AWFY's check fails.
import runner

val rap-benchmark-minified = r##"{"head":{"requestCounter":4},"operations":[["destroy","w54"],["set","w2",{"activeControl":"w99"}],["set","w21",{"customVariant":"variant_navigation"}],["set","w28",{"customVariant":"variant_selected"}],["set","w53",{"children":["w95"]}],["create","w95","rwt.widgets.Composite",{"parent":"w53","style":["NONE"],"bounds":[0,0,1008,586],"children":["w96","w97"],"tabIndex":-1,"clientArea":[0,0,1008,586]}],["create","w96","rwt.widgets.Label",{"parent":"w95","style":["NONE"],"bounds":[10,30,112,26],"tabIndex":-1,"customVariant":"variant_pageHeadline","text":"TableViewer"}],["create","w97","rwt.widgets.Composite",{"parent":"w95","style":["NONE"],"bounds":[0,61,1008,525],"children":["w98","w99","w226","w228"],"tabIndex":-1,"clientArea":[0,0,1008,525]}],["create","w98","rwt.widgets.Text",{"parent":"w97","style":["LEFT","SINGLE","BORDER"],"bounds":[10,10,988,32],"tabIndex":22,"activeKeys":["#13","#27","#40"]}],["listen","w98",{"KeyDown":true,"Modify":true}],["create","w99","rwt.widgets.Grid",{"parent":"w97","style":["SINGLE","BORDER"],"appearance":"table","indentionWidth":0,"treeColumn":-1,"markupEnabled":false}],["create","w100","rwt.widgets.ScrollBar",{"parent":"w99","style":["HORIZONTAL"]}],["create","w101","rwt.widgets.ScrollBar",{"parent":"w99","style":["VERTICAL"]}],["set","w99",{"bounds":[10,52,988,402],"children":[],"tabIndex":23,"activeKeys":["CTRL+#70","CTRL+#78","CTRL+#82","CTRL+#89","CTRL+#83","CTRL+#71","CTRL+#69"],"cancelKeys":["CTRL+#70","CTRL+#78","CTRL+#82","CTRL+#89","CTRL+#83","CTRL+#71","CTRL+#69"]}],["listen","w99",{"MouseDown":true,"MouseUp":true,"MouseDoubleClick":true,"KeyDown":true}],["set","w99",{"itemCount":118,"itemHeight":28,"itemMetrics":[[0,0,50,3,0,3,44],[1,50,50,53,0,53,44],[2,100,140,103,0,103,134],[3,240,180,243,0,243,174],[4,420,50,423,0,423,44],[5,470,50,473,0,473,44]],"columnCount":6,"headerHeight":35,"headerVisible":true,"linesVisible":true,"focusItem":"w108","selection":["w108"]}],["listen","w99",{"Selection":true,"DefaultSelection":true}],["set","w99",{"enableCellToolTip":true}],["listen","w100",{"Selection":true}],["set","w101",{"visibility":true}],["listen","w101",{"Selection":true}],["create","w102","rwt.widgets.GridColumn",{"parent":"w99","text":"Nr.","width":50,"moveable":true}],["listen","w102",{"Selection":true}],["create","w103","rwt.widgets.GridColumn",{"parent":"w99","text":"Sym.","index":1,"left":50,"width":50,"moveable":true}],["listen","w103",{"Selection":true}],["create","w104","rwt.widgets.GridColumn",{"parent":"w99","text":"Name","index":2,"left":100,"width":140,"moveable":true}],["listen","w104",{"Selection":true}],["create","w105","rwt.widgets.GridColumn",{"parent":"w99","text":"Series","index":3,"left":240,"width":180,"moveable":true}],["listen","w105",{"Selection":true}],["create","w106","rwt.widgets.GridColumn",{"parent":"w99","text":"Group","index":4,"left":420,"width":50,"moveable":true}],["listen","w106",{"Selection":true}],["create","w107","rwt.widgets.GridColumn",{"parent":"w99","text":"Period","index":5,"left":470,"width":50,"moveable":true}],["listen","w107",{"Selection":true}],["create","w108","rwt.widgets.GridItem",{"parent":"w99","index":0,"texts":["1","H","Hydrogen","Nonmetal","1","1"],"cellBackgrounds":[null,null,null,[138,226,52,255],null,null]}],["create","w109","rwt.widgets.GridItem",{"parent":"w99","index":1,"texts":["2","He","Helium","Noble gas","18","1"],"cellBackgrounds":[null,null,null,[114,159,207,255],null,null]}],["create","w110","rwt.widgets.GridItem",{"parent":"w99","index":2,"texts":["3","Li","Lithium","Alkali metal","1","2"],"cellBackgrounds":[null,null,null,[239,41,41,255],null,null]}],["create","w111","rwt.widgets.GridItem",{"parent":"w99","index":3,"texts":["4","Be","Beryllium","Alkaline earth metal","2","2"],"cellBackgrounds":[null,null,null,[233,185,110,255],null,null]}],["create","w112","rwt.widgets.GridItem",{"parent":"w99","index":4,"texts":["5","B","Boron","Metalloid","13","2"],"cellBackgrounds":[null,null,null,[156,159,153,255],null,null]}],["create","w113","rwt.widgets.GridItem",{"parent":"w99","index":5,"texts":["6","C","Carbon","Nonmetal","14","2"],"cellBackgrounds":[null,null,null,[138,226,52,255],null,null]}],["create","w114","rwt.widgets.GridItem",{"parent":"w99","index":6,"texts":["7","N","Nitrogen","Nonmetal","15","2"],"cellBackgrounds":[null,null,null,[138,226,52,255],null,null]}],["create","w115","rwt.widgets.GridItem",{"parent":"w99","index":7,"texts":["8","O","Oxygen","Nonmetal","16","2"],"cellBackgrounds":[null,null,null,[138,226,52,255],null,null]}],["create","w116","rwt.widgets.GridItem",{"parent":"w99","index":8,"texts":["9","F","Fluorine","Halogen","17","2"],"cellBackgrounds":[null,null,null,[252,233,79,255],null,null]}],["create","w117","rwt.widgets.GridItem",{"parent":"w99","index":9,"texts":["10","Ne","Neon","Noble gas","18","2"],"cellBackgrounds":[null,null,null,[114,159,207,255],null,null]}],["create","w118","rwt.widgets.GridItem",{"parent":"w99","index":10,"texts":["11","Na","Sodium","Alkali metal","1","3"],"cellBackgrounds":[null,null,null,[239,41,41,255],null,null]}],["create","w119","rwt.widgets.GridItem",{"parent":"w99","index":11,"texts":["12","Mg","Magnesium","Alkaline earth metal","2","3"],"cellBackgrounds":[null,null,null,[233,185,110,255],null,null]}],["create","w120","rwt.widgets.GridItem",{"parent":"w99","index":12,"texts":["13","Al","Aluminium","Poor metal","13","3"],"cellBackgrounds":[null,null,null,[238,238,236,255],null,null]}],["create","w121","rwt.widgets.GridItem",{"parent":"w99","index":13,"texts":["14","Si","Silicon","Metalloid","14","3"],"cellBackgrounds":[null,null,null,[156,159,153,255],null,null]}],["create","w122","rwt.widgets.GridItem",{"parent":"w99","index":14,"texts":["15","P","Phosphorus","Nonmetal","15","3"],"cellBackgrounds":[null,null,null,[138,226,52,255],null,null]}],["create","w123","rwt.widgets.GridItem",{"parent":"w99","index":15,"texts":["16","S","Sulfur","Nonmetal","16","3"],"cellBackgrounds":[null,null,null,[138,226,52,255],null,null]}],["create","w124","rwt.widgets.GridItem",{"parent":"w99","index":16,"texts":["17","Cl","Chlorine","Halogen","17","3"],"cellBackgrounds":[null,null,null,[252,233,79,255],null,null]}],["create","w125","rwt.widgets.GridItem",{"parent":"w99","index":17,"texts":["18","Ar","Argon","Noble gas","18","3"],"cellBackgrounds":[null,null,null,[114,159,207,255],null,null]}],["create","w126","rwt.widgets.GridItem",{"parent":"w99","index":18,"texts":["19","K","Potassium","Alkali metal","1","4"],"cellBackgrounds":[null,null,null,[239,41,41,255],null,null]}],["create","w127","rwt.widgets.GridItem",{"parent":"w99","index":19,"texts":["20","Ca","Calcium","Alkaline earth metal","2","4"],"cellBackgrounds":[null,null,null,[233,185,110,255],null,null]}],["create","w128","rwt.widgets.GridItem",{"parent":"w99","index":20,"texts":["21","Sc","Scandium","Transition metal","3","4"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w129","rwt.widgets.GridItem",{"parent":"w99","index":21,"texts":["22","Ti","Titanium","Transition metal","4","4"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w130","rwt.widgets.GridItem",{"parent":"w99","index":22,"texts":["23","V","Vanadium","Transition metal","5","4"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w131","rwt.widgets.GridItem",{"parent":"w99","index":23,"texts":["24","Cr","Chromium","Transition metal","6","4"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w132","rwt.widgets.GridItem",{"parent":"w99","index":24,"texts":["25","Mn","Manganese","Transition metal","7","4"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w133","rwt.widgets.GridItem",{"parent":"w99","index":25,"texts":["26","Fe","Iron","Transition metal","8","4"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w134","rwt.widgets.GridItem",{"parent":"w99","index":26,"texts":["27","Co","Cobalt","Transition metal","9","4"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w135","rwt.widgets.GridItem",{"parent":"w99","index":27,"texts":["28","Ni","Nickel","Transition metal","10","4"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w136","rwt.widgets.GridItem",{"parent":"w99","index":28,"texts":["29","Cu","Copper","Transition metal","11","4"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w137","rwt.widgets.GridItem",{"parent":"w99","index":29,"texts":["30","Zn","Zinc","Transition metal","12","4"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w138","rwt.widgets.GridItem",{"parent":"w99","index":30,"texts":["31","Ga","Gallium","Poor metal","13","4"],"cellBackgrounds":[null,null,null,[238,238,236,255],null,null]}],["create","w139","rwt.widgets.GridItem",{"parent":"w99","index":31,"texts":["32","Ge","Germanium","Metalloid","14","4"],"cellBackgrounds":[null,null,null,[156,159,153,255],null,null]}],["create","w140","rwt.widgets.GridItem",{"parent":"w99","index":32,"texts":["33","As","Arsenic","Metalloid","15","4"],"cellBackgrounds":[null,null,null,[156,159,153,255],null,null]}],["create","w141","rwt.widgets.GridItem",{"parent":"w99","index":33,"texts":["34","Se","Selenium","Nonmetal","16","4"],"cellBackgrounds":[null,null,null,[138,226,52,255],null,null]}],["create","w142","rwt.widgets.GridItem",{"parent":"w99","index":34,"texts":["35","Br","Bromine","Halogen","17","4"],"cellBackgrounds":[null,null,null,[252,233,79,255],null,null]}],["create","w143","rwt.widgets.GridItem",{"parent":"w99","index":35,"texts":["36","Kr","Krypton","Noble gas","18","4"],"cellBackgrounds":[null,null,null,[114,159,207,255],null,null]}],["create","w144","rwt.widgets.GridItem",{"parent":"w99","index":36,"texts":["37","Rb","Rubidium","Alkali metal","1","5"],"cellBackgrounds":[null,null,null,[239,41,41,255],null,null]}],["create","w145","rwt.widgets.GridItem",{"parent":"w99","index":37,"texts":["38","Sr","Strontium","Alkaline earth metal","2","5"],"cellBackgrounds":[null,null,null,[233,185,110,255],null,null]}],["create","w146","rwt.widgets.GridItem",{"parent":"w99","index":38,"texts":["39","Y","Yttrium","Transition metal","3","5"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w147","rwt.widgets.GridItem",{"parent":"w99","index":39,"texts":["40","Zr","Zirconium","Transition metal","4","5"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w148","rwt.widgets.GridItem",{"parent":"w99","index":40,"texts":["41","Nb","Niobium","Transition metal","5","5"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w149","rwt.widgets.GridItem",{"parent":"w99","index":41,"texts":["42","Mo","Molybdenum","Transition metal","6","5"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w150","rwt.widgets.GridItem",{"parent":"w99","index":42,"texts":["43","Tc","Technetium","Transition metal","7","5"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w151","rwt.widgets.GridItem",{"parent":"w99","index":43,"texts":["44","Ru","Ruthenium","Transition metal","8","5"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w152","rwt.widgets.GridItem",{"parent":"w99","index":44,"texts":["45","Rh","Rhodium","Transition metal","9","5"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w153","rwt.widgets.GridItem",{"parent":"w99","index":45,"texts":["46","Pd","Palladium","Transition metal","10","5"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w154","rwt.widgets.GridItem",{"parent":"w99","index":46,"texts":["47","Ag","Silver","Transition metal","11","5"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w155","rwt.widgets.GridItem",{"parent":"w99","index":47,"texts":["48","Cd","Cadmium","Transition metal","12","5"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w156","rwt.widgets.GridItem",{"parent":"w99","index":48,"texts":["49","In","Indium","Poor metal","13","5"],"cellBackgrounds":[null,null,null,[238,238,236,255],null,null]}],["create","w157","rwt.widgets.GridItem",{"parent":"w99","index":49,"texts":["50","Sn","Tin","Poor metal","14","5"],"cellBackgrounds":[null,null,null,[238,238,236,255],null,null]}],["create","w158","rwt.widgets.GridItem",{"parent":"w99","index":50,"texts":["51","Sb","Antimony","Metalloid","15","5"],"cellBackgrounds":[null,null,null,[156,159,153,255],null,null]}],["create","w159","rwt.widgets.GridItem",{"parent":"w99","index":51,"texts":["52","Te","Tellurium","Metalloid","16","5"],"cellBackgrounds":[null,null,null,[156,159,153,255],null,null]}],["create","w160","rwt.widgets.GridItem",{"parent":"w99","index":52,"texts":["53","I","Iodine","Halogen","17","5"],"cellBackgrounds":[null,null,null,[252,233,79,255],null,null]}],["create","w161","rwt.widgets.GridItem",{"parent":"w99","index":53,"texts":["54","Xe","Xenon","Noble gas","18","5"],"cellBackgrounds":[null,null,null,[114,159,207,255],null,null]}],["create","w162","rwt.widgets.GridItem",{"parent":"w99","index":54,"texts":["55","Cs","Caesium","Alkali metal","1","6"],"cellBackgrounds":[null,null,null,[239,41,41,255],null,null]}],["create","w163","rwt.widgets.GridItem",{"parent":"w99","index":55,"texts":["56","Ba","Barium","Alkaline earth metal","2","6"],"cellBackgrounds":[null,null,null,[233,185,110,255],null,null]}],["create","w164","rwt.widgets.GridItem",{"parent":"w99","index":56,"texts":["57","La","Lanthanum","Lanthanide","3","6"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w165","rwt.widgets.GridItem",{"parent":"w99","index":57,"texts":["58","Ce","Cerium","Lanthanide","3","6"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w166","rwt.widgets.GridItem",{"parent":"w99","index":58,"texts":["59","Pr","Praseodymium","Lanthanide","3","6"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w167","rwt.widgets.GridItem",{"parent":"w99","index":59,"texts":["60","Nd","Neodymium","Lanthanide","3","6"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w168","rwt.widgets.GridItem",{"parent":"w99","index":60,"texts":["61","Pm","Promethium","Lanthanide","3","6"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w169","rwt.widgets.GridItem",{"parent":"w99","index":61,"texts":["62","Sm","Samarium","Lanthanide","3","6"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w170","rwt.widgets.GridItem",{"parent":"w99","index":62,"texts":["63","Eu","Europium","Lanthanide","3","6"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w171","rwt.widgets.GridItem",{"parent":"w99","index":63,"texts":["64","Gd","Gadolinium","Lanthanide","3","6"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w172","rwt.widgets.GridItem",{"parent":"w99","index":64,"texts":["65","Tb","Terbium","Lanthanide","3","6"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w173","rwt.widgets.GridItem",{"parent":"w99","index":65,"texts":["66","Dy","Dysprosium","Lanthanide","3","6"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w174","rwt.widgets.GridItem",{"parent":"w99","index":66,"texts":["67","Ho","Holmium","Lanthanide","3","6"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w175","rwt.widgets.GridItem",{"parent":"w99","index":67,"texts":["68","Er","Erbium","Lanthanide","3","6"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w176","rwt.widgets.GridItem",{"parent":"w99","index":68,"texts":["69","Tm","Thulium","Lanthanide","3","6"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w177","rwt.widgets.GridItem",{"parent":"w99","index":69,"texts":["70","Yb","Ytterbium","Lanthanide","3","6"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w178","rwt.widgets.GridItem",{"parent":"w99","index":70,"texts":["71","Lu","Lutetium","Lanthanide","3","6"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w179","rwt.widgets.GridItem",{"parent":"w99","index":71,"texts":["72","Hf","Hafnium","Transition metal","4","6"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w180","rwt.widgets.GridItem",{"parent":"w99","index":72,"texts":["73","Ta","Tantalum","Transition metal","5","6"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w181","rwt.widgets.GridItem",{"parent":"w99","index":73,"texts":["74","W","Tungsten","Transition metal","6","6"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w182","rwt.widgets.GridItem",{"parent":"w99","index":74,"texts":["75","Re","Rhenium","Transition metal","7","6"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w183","rwt.widgets.GridItem",{"parent":"w99","index":75,"texts":["76","Os","Osmium","Transition metal","8","6"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w184","rwt.widgets.GridItem",{"parent":"w99","index":76,"texts":["77","Ir","Iridium","Transition metal","9","6"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w185","rwt.widgets.GridItem",{"parent":"w99","index":77,"texts":["78","Pt","Platinum","Transition metal","10","6"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w186","rwt.widgets.GridItem",{"parent":"w99","index":78,"texts":["79","Au","Gold","Transition metal","11","6"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w187","rwt.widgets.GridItem",{"parent":"w99","index":79,"texts":["80","Hg","Mercury","Transition metal","12","6"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w188","rwt.widgets.GridItem",{"parent":"w99","index":80,"texts":["81","Tl","Thallium","Poor metal","13","6"],"cellBackgrounds":[null,null,null,[238,238,236,255],null,null]}],["create","w189","rwt.widgets.GridItem",{"parent":"w99","index":81,"texts":["82","Pb","Lead","Poor metal","14","6"],"cellBackgrounds":[null,null,null,[238,238,236,255],null,null]}],["create","w190","rwt.widgets.GridItem",{"parent":"w99","index":82,"texts":["83","Bi","Bismuth","Poor metal","15","6"],"cellBackgrounds":[null,null,null,[238,238,236,255],null,null]}],["create","w191","rwt.widgets.GridItem",{"parent":"w99","index":83,"texts":["84","Po","Polonium","Metalloid","16","6"],"cellBackgrounds":[null,null,null,[156,159,153,255],null,null]}],["create","w192","rwt.widgets.GridItem",{"parent":"w99","index":84,"texts":["85","At","Astatine","Halogen","17","6"],"cellBackgrounds":[null,null,null,[252,233,79,255],null,null]}],["create","w193","rwt.widgets.GridItem",{"parent":"w99","index":85,"texts":["86","Rn","Radon","Noble gas","18","6"],"cellBackgrounds":[null,null,null,[114,159,207,255],null,null]}],["create","w194","rwt.widgets.GridItem",{"parent":"w99","index":86,"texts":["87","Fr","Francium","Alkali metal","1","7"],"cellBackgrounds":[null,null,null,[239,41,41,255],null,null]}],["create","w195","rwt.widgets.GridItem",{"parent":"w99","index":87,"texts":["88","Ra","Radium","Alkaline earth metal","2","7"],"cellBackgrounds":[null,null,null,[233,185,110,255],null,null]}],["create","w196","rwt.widgets.GridItem",{"parent":"w99","index":88,"texts":["89","Ac","Actinium","Actinide","3","7"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w197","rwt.widgets.GridItem",{"parent":"w99","index":89,"texts":["90","Th","Thorium","Actinide","3","7"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w198","rwt.widgets.GridItem",{"parent":"w99","index":90,"texts":["91","Pa","Protactinium","Actinide","3","7"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w199","rwt.widgets.GridItem",{"parent":"w99","index":91,"texts":["92","U","Uranium","Actinide","3","7"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w200","rwt.widgets.GridItem",{"parent":"w99","index":92,"texts":["93","Np","Neptunium","Actinide","3","7"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w201","rwt.widgets.GridItem",{"parent":"w99","index":93,"texts":["94","Pu","Plutonium","Actinide","3","7"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w202","rwt.widgets.GridItem",{"parent":"w99","index":94,"texts":["95","Am","Americium","Actinide","3","7"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w203","rwt.widgets.GridItem",{"parent":"w99","index":95,"texts":["96","Cm","Curium","Actinide","3","7"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w204","rwt.widgets.GridItem",{"parent":"w99","index":96,"texts":["97","Bk","Berkelium","Actinide","3","7"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w205","rwt.widgets.GridItem",{"parent":"w99","index":97,"texts":["98","Cf","Californium","Actinide","3","7"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w206","rwt.widgets.GridItem",{"parent":"w99","index":98,"texts":["99","Es","Einsteinium","Actinide","3","7"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w207","rwt.widgets.GridItem",{"parent":"w99","index":99,"texts":["100","Fm","Fermium","Actinide","3","7"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w208","rwt.widgets.GridItem",{"parent":"w99","index":100,"texts":["101","Md","Mendelevium","Actinide","3","7"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w209","rwt.widgets.GridItem",{"parent":"w99","index":101,"texts":["102","No","Nobelium","Actinide","3","7"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w210","rwt.widgets.GridItem",{"parent":"w99","index":102,"texts":["103","Lr","Lawrencium","Actinide","3","7"],"cellBackgrounds":[null,null,null,[173,127,168,255],null,null]}],["create","w211","rwt.widgets.GridItem",{"parent":"w99","index":103,"texts":["104","Rf","Rutherfordium","Transition metal","4","7"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w212","rwt.widgets.GridItem",{"parent":"w99","index":104,"texts":["105","Db","Dubnium","Transition metal","5","7"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w213","rwt.widgets.GridItem",{"parent":"w99","index":105,"texts":["106","Sg","Seaborgium","Transition metal","6","7"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w214","rwt.widgets.GridItem",{"parent":"w99","index":106,"texts":["107","Bh","Bohrium","Transition metal","7","7"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w215","rwt.widgets.GridItem",{"parent":"w99","index":107,"texts":["108","Hs","Hassium","Transition metal","8","7"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w216","rwt.widgets.GridItem",{"parent":"w99","index":108,"texts":["109","Mt","Meitnerium","Transition metal","9","7"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w217","rwt.widgets.GridItem",{"parent":"w99","index":109,"texts":["110","Ds","Darmstadtium","Transition metal","10","7"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w218","rwt.widgets.GridItem",{"parent":"w99","index":110,"texts":["111","Rg","Roentgenium","Transition metal","11","7"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w219","rwt.widgets.GridItem",{"parent":"w99","index":111,"texts":["112","Uub","Ununbium","Transition metal","12","7"],"cellBackgrounds":[null,null,null,[252,175,62,255],null,null]}],["create","w220","rwt.widgets.GridItem",{"parent":"w99","index":112,"texts":["113","Uut","Ununtrium","Poor metal","13","7"],"cellBackgrounds":[null,null,null,[238,238,236,255],null,null]}],["create","w221","rwt.widgets.GridItem",{"parent":"w99","index":113,"texts":["114","Uuq","Ununquadium","Poor metal","14","7"],"cellBackgrounds":[null,null,null,[238,238,236,255],null,null]}],["create","w222","rwt.widgets.GridItem",{"parent":"w99","index":114,"texts":["115","Uup","Ununpentium","Poor metal","15","7"],"cellBackgrounds":[null,null,null,[238,238,236,255],null,null]}],["create","w223","rwt.widgets.GridItem",{"parent":"w99","index":115,"texts":["116","Uuh","Ununhexium","Poor metal","16","7"],"cellBackgrounds":[null,null,null,[238,238,236,255],null,null]}],["create","w224","rwt.widgets.GridItem",{"parent":"w99","index":116,"texts":["117","Uus","Ununseptium","Halogen","17","7"],"cellBackgrounds":[null,null,null,[252,233,79,255],null,null]}],["create","w225","rwt.widgets.GridItem",{"parent":"w99","index":117,"texts":["118","Uuo","Ununoctium","Noble gas","18","7"],"cellBackgrounds":[null,null,null,[114,159,207,255],null,null]}],["create","w226","rwt.widgets.Composite",{"parent":"w97","style":["BORDER"],"bounds":[10,464,988,25],"children":["w227"],"tabIndex":-1,"clientArea":[0,0,986,23]}],["create","w227","rwt.widgets.Label",{"parent":"w226","style":["NONE"],"bounds":[10,10,966,3],"tabIndex":-1,"text":"Hydrogen (H)"}],["create","w228","rwt.widgets.Label",{"parent":"w97","style":["WRAP"],"bounds":[10,499,988,16],"tabIndex":-1,"foreground":[150,150,150,255],"font":[["Verdana","Lucida Sans","Arial","Helvetica","sans-serif"],10,false,false],"text":"Shortcuts: [CTRL+F] - Filter | Sort by: [CTRL+R] - Number, [CTRL+Y] - Symbol, [CTRL+N] - Name, [CTRL+S] - Series, [CTRL+G] - Group, [CTRL+E] - Period"}],["set","w1",{"focusControl":"w99"}],["call","rwt.client.BrowserNavigation","addToHistory",{"entries":[["tableviewer","TableViewer"]]}]]}"##

fun string-hash(s: string): int
  // this is not a proper hash, but sufficient for the benchmark,
  // and very portable!
  s.bytes.length * 1402589

// ---- som's Vector (the part Json uses): `storage` holds `last-idx` entries and spare capacity

val initial-size = 10

struct vec<a>
  storage: vector<a>
  last-idx: int

fun vec/empty(): vec<a>
  Vec([].vector, 0)

fun vec/at(v: vec<a>, idx: int): exn a
  v.storage[idx]

fun vec/size(v: vec<a>): int
  v.last-idx

fun vec/append(v: vec<a>, elem: a): exn vec<a>
  match v
    Vec(storage, last) ->
      if last < storage.length then
        var xs := storage
        xs[last] := elem
        Vec(xs, last + 1)
      else
        val capacity = if last == 0 then initial-size else 2 * last
        val grown = vector-init-total(capacity) fn(i)
          if i < last then storage.at(i).default(elem) else elem
        Vec(grown, last + 1)

// ---- HashIndexTable: 32 slots (a power of two), each 0 for empty or index + 1

fun hash-slot-for(table: vector<int>, element: string): int
  string-hash(element) % table.length

fun table-add(table: vector<int>, name: string, index: int): exn vector<int>
  val slot = hash-slot-for(table, name)
  var t := table
  // increment by 1, 0 stands for empty
  t[slot] := (if index < 0xff then (index + 1) % 256 else 0)
  t

fun table-get(table: vector<int>, name: string): exn int
  val slot = hash-slot-for(table, name)
  // subtract 1, 0 stands for empty
  table[slot] % 256 - 1

// ---- values

type json-value
  JObject(o: json-object)
  JArray(values: vec<json-value>)
  JNumber(string: string)
  JString(string: string)
  JNull
  JTrue
  JFalse

struct json-object
  names: vec<string>
  values: vec<json-value>
  table: vector<int>

fun object/empty(): json-object
  Json-object(vec/empty(), vec/empty(), vector(32, 0))

// Destructure first so the vectors stay uniquely owned and update in place.
fun object/add(o: json-object, name: string, value: json-value): exn json-object
  match o
    Json-object(names, values, table) ->
      val t = table-add(table, name, names.size)
      Json-object(names.append(name), values.append(value), t)

fun object/index-of(o: json-object, name: string): exn int
  val index = table-get(o.table, name)
  if index != -1 && name == o.names.at(index) then index
  else throw("not implemented") // Not needed for benchmark

fun object/get(o: json-object, name: string): exn maybe<json-value>
  val index = o.index-of(name)
  if index == -1 then Nothing else Just(o.values.at(index))

// ---- the parser: a value threaded through every call, which Perceus updates in place

struct parser
  input: bytes
  index: int
  line: int
  column: int
  current: int // -1 is AWFY's null
  capture-start: int
  capture-buffer: string

fun new-parser(s: string): parser
  Parser(s.bytes, -1, 1, 0, -1, -1, "")

fun parse(p0: parser): <div,exn> json-value
  val p1 = p0.read.skip-white-space
  val (p2, result) = p1.read-value
  val p3 = p2.skip-white-space
  if !p3.is-end-of-text then p3.error("Unexpected character")
  result

fun read-value(p: parser): <div,exn> (parser, json-value)
  val c = p.current
  if c == 'n'.int then p.read-null
  elif c == 't'.int then p.read-true
  elif c == 'f'.int then p.read-false
  elif c == '"'.int then p.read-string
  elif c == '['.int then p.read-array
  elif c == '{'.int then p.read-object
  elif c == '-'.int || (c >= '0'.int && c <= '9'.int) then p.read-number
  else p.expected("value")

fun read-array(p0: parser): <div,exn> (parser, json-value)
  val (p1, empty) = p0.read.skip-white-space.read-char(']')
  if empty then return (p1, JArray(vec/empty()))
  val (p2, array) = p1.read-elements(vec/empty())
  val (p3, closed) = p2.read-char(']')
  if !closed then p3.expected("',' or ']'")
  (p3, JArray(array))

// AWFY's do-while loop over `,`-separated values, the vector passed on so it stays unique.
fun read-elements(p0: parser, array: vec<json-value>): <div,exn> (parser, vec<json-value>)
  val (p1, value) = p0.skip-white-space.read-value
  val more = array.append(value)
  val (p2, comma) = p1.skip-white-space.read-char(',')
  if comma then p2.read-elements(more) else (p2, more)

fun read-object(p0: parser): <div,exn> (parser, json-value)
  val (p1, empty) = p0.read.skip-white-space.read-char('}')
  if empty then return (p1, JObject(object/empty()))
  val (p2, object) = p1.read-members(object/empty())
  val (p3, closed) = p2.read-char('}')
  if !closed then p3.expected("',' or '}'")
  (p3, JObject(object))

// AWFY's do-while loop over `,`-separated members.
fun read-members(p0: parser, object: json-object): <div,exn> (parser, json-object)
  val (p1, name) = p0.skip-white-space.read-name
  val (p2, colon) = p1.skip-white-space.read-char(':')
  if !colon then p2.expected("':'")
  val (p3, value) = p2.skip-white-space.read-value
  val more = object.add(name, value)
  val (p4, comma) = p3.skip-white-space.read-char(',')
  if comma then p4.read-members(more) else (p4, more)

fun read-name(p: parser): <div,exn> (parser, string)
  if p.current != '"'.int then p.expected("name")
  p.read-string-internal

fun read-null(p: parser): exn (parser, json-value)
  (p.read.read-required-char('u').read-required-char('l').read-required-char('l'), JNull)

fun read-true(p: parser): exn (parser, json-value)
  (p.read.read-required-char('r').read-required-char('u').read-required-char('e'), JTrue)

fun read-false(p: parser): exn (parser, json-value)
  val q = p.read.read-required-char('a').read-required-char('l')
  (q.read-required-char('s').read-required-char('e'), JFalse)

fun read-required-char(p: parser, ch: char): exn parser
  val (q, ok) = p.read-char(ch)
  if !ok then q.expected("'" ++ ch.string ++ "'")
  q

fun read-string(p: parser): <div,exn> (parser, json-value)
  val (q, s) = p.read-string-internal
  (q, JString(s))

fun read-string-internal(p: parser): <div,exn> (parser, string)
  val (q, s) = p.read.start-capture.read-string-chars.end-capture
  (q.read, s)

// AWFY's `while (this.current !== '"')` loop.
fun read-string-chars(p: parser): <div,exn> parser
  if p.current == '"'.int then p
  elif p.current == '\\'.int then p.pause-capture.read-escape.start-capture.read-string-chars
  else p.read.read-string-chars

fun read-escape(p0: parser): exn parser
  val p = p0.read
  val c = p.current
  val add =
    if c == '"'.int || c == '/'.int || c == '\\'.int then c.char.string
    elif c == 'b'.int then 8.char.string
    elif c == 'f'.int then 12.char.string
    elif c == 'n'.int then "\n"
    elif c == 'r'.int then "\r"
    elif c == 't'.int then "\t"
    else p.expected("valid escape sequence")
  p(capture-buffer = p.capture-buffer ++ add).read

fun read-number(p0: parser): <div,exn> (parser, json-value)
  val (p1, _) = p0.start-capture.read-char('-')
  val first-digit = p1.current
  val (p2, digit) = p1.read-digit
  if !digit then p2.expected("digit")
  val p3 = if first-digit != '0'.int then p2.read-digits else p2
  val (p4, s) = p3.read-fraction.read-exponent.end-capture
  (p4, JNumber(s))

fun read-fraction(p0: parser): <div,exn> parser
  val (p1, dot) = p0.read-char('.')
  if !dot then return p1
  val (p2, digit) = p1.read-digit
  if !digit then p2.expected("digit")
  p2.read-digits

fun read-exponent(p0: parser): <div,exn> parser
  val (p1, e) = p0.read-char('e')
  val (p2, big-e) = if e then (p1, False) else p1.read-char('E')
  if !e && !big-e then return p2
  val (p3, plus) = p2.read-char('+')
  val p4 = if plus then p3 else p3.read-char('-').fst
  val (p5, digit) = p4.read-digit
  if !digit then p5.expected("digit")
  p5.read-digits

// AWFY's `while (this.readDigit()) {}`.
fun read-digits(p0: parser): div parser
  val (p1, digit) = p0.read-digit
  if digit then p1.read-digits else p1

fun read-char(p: parser, ch: char): (parser, bool)
  if p.current != ch.int then (p, False) else (p.read, True)

fun read-digit(p: parser): (parser, bool)
  if !p.is-digit then (p, False) else (p.read, True)

fun skip-white-space(p: parser): div parser
  if p.is-white-space then p.read.skip-white-space else p

fun read(p: parser): parser
  match p
    Parser(input, index, line, column, current, capture-start, capture-buffer) ->
      val newline = current == '\n'.int
      val i = index + 1
      val c = if i < input.length then input.unsafe-index(i.ssize_t).int else -1
      Parser(input, i, if newline then line + 1 else line, if newline then 0 else column, c,
             capture-start, capture-buffer)

fun start-capture(p: parser): parser
  p(capture-start = p.index)

// AWFY's `end + 1`, kept inside the input as JS's substring clamps it.
fun capture-end(p: parser): int
  if p.current == -1 then p.input.length else p.index

fun pause-capture(p: parser): parser
  val start = p.capture-start
  val s = p.input.substring(start, p.capture-end - start)
  p(capture-buffer = p.capture-buffer ++ s, capture-start = -1)

fun end-capture(p: parser): (parser, string)
  val start = p.capture-start
  val s = p.input.substring(start, p.capture-end - start)
  if p.capture-buffer == "" then (p(capture-start = -1), s)
  else (p(capture-buffer = "", capture-start = -1), p.capture-buffer ++ s)

fun expected(p: parser, expected: string): exn a
  if p.is-end-of-text then p.error("Unexpected end of input")
  p.error("Expected " ++ expected)

fun error(p: parser, message: string): exn a
  throw(message ++ " at " ++ p.line.show ++ ":" ++ (p.column - 1).show)

fun is-white-space(p: parser): bool
  val c = p.current
  c == ' '.int || c == '\t'.int || c == '\n'.int || c == '\r'.int

fun is-digit(p: parser): bool
  p.current >= '0'.int && p.current <= '9'.int

fun is-end-of-text(p: parser): bool
  p.current == -1

// ---- the benchmark

fun benchmark(): <div,exn> json-value
  new-parser(rap-benchmark-minified).parse

// verifyResult's checks; the encoded result is the `operations` array's size, 0 on a failure.
fun verify-result(result: json-value): exn int
  match result
    JObject(o) -> match o.get("head")
      Just(JObject) -> match o.get("operations")
        Just(JArray(operations)) -> operations.size
        _ -> 0
      _ -> 0
    _ -> 0

fun json(n: int): <div,exn> int
  var total := 0
  for(n, fn(_) { total := total + verify-result(benchmark()) })
  total

fun main() { bench-main(json, 1000) }
