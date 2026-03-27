use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "recipe/endf_recipe.pest"]
struct RecipeParser;

#[test]
fn parse_tapehead_mf0_mt0() {
    let input = "\n[MAT, 0, 0/ TAPEDESCR]TEXT\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse tapehead: {:?}", result.err());
}

#[test]
fn parse_head_record() {
    let input = "\n[MAT, 2,151/ ZA, AWR, 0, 0, NIS, 0]HEAD\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse HEAD record: {:?}", result.err());
}

#[test]
fn parse_cont_record() {
    let input = "\n[MAT, 2,151/ EL, EH, LRU, LRF, NRO, NAPS]CONT\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse CONT record: {:?}", result.err());
}

#[test]
fn parse_tab1_record() {
    let input = "\n[MAT, 2,151/ 0.0, 0.0, 0, 0, NR, NP/ Eint / AP]TAB1 (AP_table)\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse TAB1 record: {:?}", result.err());
}

#[test]
fn parse_tab2_record() {
    let input = "\n[MAT, 1,455/ 0.0, 0.0, 0, 0, NR, NE/ Eint ]TAB2\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse TAB2 record: {:?}", result.err());
}

#[test]
fn parse_list_record_simple() {
    let input = "\n[MAT,2,151/ ED, EU, 0, 0, 5, 0/ R0, R1, R2, S0, S1 ]LIST\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse simple LIST record: {:?}", result.err());
}

#[test]
fn parse_list_record_with_loop() {
    let input = "\n[MAT, 2,151/ AWRI, QX, L, LRX, 6*NRS, NRS /\n{ER[k], AJ[k], GT[k], GN[k], GG[k], GF[k]}{k=1 to NRS} ]LIST\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse LIST with loop: {:?}", result.err());
}

#[test]
fn parse_send() {
    let input = "\nSEND\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse SEND: {:?}", result.err());
}

#[test]
fn parse_for_loop() {
    let input = "\nfor i=1 to NIS:\n[MAT, 2,151/ ZAI, ABN, 0, LFW, NER, 0]CONT\nendfor\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse for loop: {:?}", result.err());
}

#[test]
fn parse_if_clause() {
    let input = "\nif LRU==0 and LRF==0:\n[MAT, 2,151/ SPI, AP, 0, 0, 0, 0]CONT\nendif\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse if clause: {:?}", result.err());
}

#[test]
fn parse_if_elif_else() {
    let input = r#"
if LRF==1 or LRF==2:
[MAT, 2,151/ SPI, AP, 0, 0, NLS, 0]CONT
elif LRF==3:
[MAT, 2,151/ SPI, AP, LAD, 0, NLS, NLSC]CONT
else:
[MAT, 2,151/ SPI, AP, 0, 0, 0, 0]CONT
endif
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse if/elif/else: {:?}", result.err());
}

#[test]
fn parse_comment_block() {
    let input = "# This is a comment\n# Another comment line\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse comment block: {:?}", result.err());
}

#[test]
fn parse_section() {
    let input = "\n(isotope[i])\n[MAT, 2,151/ ZAI, ABN, 0, LFW, NER, 0]CONT\n(/isotope[i])\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse section: {:?}", result.err());
}

#[test]
fn parse_abbreviation() {
    let input = "\nNX := (1+NCH + (5-NCH) % 6) * NRS / 6\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse abbreviation: {:?}", result.err());
}

#[test]
fn parse_dir_record() {
    let input = "\n[MAT, 1,451/ blank, blank, MFx[i], MTx[i], NCx[i], MOD[i]]DIR\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse DIR record: {:?}", result.err());
}

#[test]
fn parse_text_record() {
    let input = "\n[MAT, 1,451/ ZSYMAM{11}, ALAB{11}, EDATE{10}, {1}, AUTH{33} ]TEXT\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse TEXT record: {:?}", result.err());
}

#[test]
fn parse_text_record_simple() {
    let input = "\n[MAT, 1,451/ HSUB[i]] TEXT\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse simple TEXT record: {:?}", result.err());
}

#[test]
fn parse_text_record_multiline() {
    let input = "\n[MAT, 1,451/ {1}, REF{21}, DDATE{10}, {1},\n             RDATE{10}, {12}, ENDATE{8}, {3} ]TEXT\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse multiline TEXT record: {:?}", result.err());
}

#[test]
fn parse_lookahead() {
    let input = "\nif NST==0 [lookahead=1]:\n[MAT, 8,457/ ZA, AWR, LIS, LISO, NST, NSP]HEAD\nendif\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse lookahead: {:?}", result.err());
}

#[test]
fn parse_stop() {
    let input = "\nstop(\"Format error: NX<1 or NX>3 for LRF=4 (Adler-Adler)\")\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse stop: {:?}", result.err());
}

#[test]
fn parse_division_in_record_field_parenthesized() {
    // Division inside parens is allowed in record fields
    let input = "\n[MAT,32,151/ 0.0, 0.0, MPAR, 0, 6*NRB+(MPAR*NRB)*(MPAR*NRB+1)/2, NRB/\n{ER[k]}{k=1 to NRB} ] LIST\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse division in record field: {:?}", result.err());
}

#[test]
fn parse_nested_list_loops() {
    let input = "\n[MAT,32,151/ 0.0, 0.0, LS, LB, NT, NE/\n{E[k]}{k=1 to NE} {{F[k,kp]}{kp=k to NE-1}}{k=1 to NE-1} ]LIST\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse nested list loops: {:?}", result.err());
}

#[test]
fn parse_repeat_loop() {
    let input = r#"
repeat [i=1]:
[MAT, 33, MT/ 0.0, 0.0, NG1[i], IG1[i], NG1[i], IG[i] /
{COV[i,j]}{j=1 to NG1[i]}] LIST
until IG[i] == NG
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse repeat loop: {:?}", result.err());
}

#[test]
fn parse_mf3_recipe() {
    let input = r#"
[MAT, 3, MT/ ZA, AWR, 0, 0, 0, 0] HEAD
[MAT, 3, MT/ QM, QI, 0, LR, NR, NP / E / xs]TAB1 (xstable)
SEND
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse MF3 recipe: {:?}", result.err());
}

#[test]
fn parse_mf0_mt0_tapehead() {
    let input = "\n\n[MAT, 0, 0/ TAPEDESCR]TEXT\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse MF0 MT0 tapehead: {:?}", result.err());
}

#[test]
fn parse_complex_boolean_condition() {
    let input = "\nif LRU==0 and LRF==0 and NRO==0 and NAPS==0 and LFW==0 and NER==1:\n[MAT, 2,151/ SPI, AP, 0, 0, 0, 0]CONT\nendif\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse complex boolean: {:?}", result.err());
}

#[test]
fn parse_disjunction_with_parens() {
    let input = "\nif NRO!=0 and (NAPS==0 or NAPS==1):\n[MAT, 2,151/ SPI, 0.0, 0, 0, NLS, 0]CONT\nendif\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse disjunction with parens: {:?}", result.err());
}

#[test]
fn parse_negative_number_in_expression() {
    let input = "\nNWD_val := NWD-5\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse subtraction: {:?}", result.err());
}

#[test]
fn parse_for_with_expression_bounds() {
    let input = "\nfor i=1 to NWD-5:\n[MAT, 1,451/ DESCRIPTION[i]]TEXT\nendfor\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse for with expr bounds: {:?}", result.err());
}

#[test]
fn parse_division_in_list_for_head() {
    // JCH/NCH division inside list_for_head (uses for_stop -> expr)
    let input = r#"
[MAT,32,151/ 0.0, 0.0, 0, 0, JCH, (1+(NCH-1)/6) /
{{DAP[m,n]}{n=1 to NCH}}{m=1 to JCH/NCH} ] LIST
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse division in list for head: {:?}", result.err());
}

#[test]
fn parse_list_with_plain_body() {
    // LIST body starting with plain variables (no { loop)
    let input = "\n[MAT,2,151/ ED, EU, 0, 0, 5, 0/ R0, R1, R2, S0, S1 ]LIST\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse LIST with plain body: {:?}", result.err());
}

#[test]
fn parse_list_with_division_in_5th_field() {
    let input = "\n[MAT,32,151/ 0.0, 0.0, IDP, LB, NEB*(NEB+1)/2, NEB/\n{E[k]}{k=1 to NEB} ]LIST\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse division in 5th field: {:?}", result.err());
}

#[test]
fn parse_full_mf1_mt451() {
    let input = r#"
# var ZA:      ZA = (1000.0 * Z) + A
[MAT, 1,451/ ZA, AWR, LRP, LFI, NLIB, NMOD]HEAD
[MAT, 1,451/ ELIS, STA, LIS, LISO, 0, NFOR]CONT
[MAT, 1,451/ AWI, EMAX, LREL, 0, NSUB, NVER]CONT
[MAT, 1,451/ TEMP, 0.0, LDRV, 0, NWD, NXC]CONT

[MAT, 1,451/ ZSYMAM{11}, ALAB{11}, EDATE{10}, {1}, AUTH{33} ]TEXT
[MAT, 1,451/ {1}, REF{21}, DDATE{10}, {1},
             RDATE{10}, {12}, ENDATE{8}, {3} ]TEXT
for i=1 to 3:
    [MAT, 1,451/ HSUB[i]] TEXT
endfor
for i=1 to NWD-5:
    [MAT, 1,451/ DESCRIPTION[i]]TEXT
endfor
for i=1 to NXC:
    [MAT, 1,451/ blank, blank, MFx[i], MTx[i], NCx[i], MOD[i]]DIR
endfor
SEND
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse MF1 MT451: {:?}", result.err());
}

#[test]
fn parse_full_mf3() {
    let input = r#"
# var ZA:   ZA = (1000.0 * Z) + A
#           Z ... charge number of material
#           A ... mass number of material
# var AWR:  ratio of the mass of the material to that of the neutron

[MAT, 3, MT/ ZA, AWR, 0, 0, 0, 0] HEAD
[MAT, 3, MT/ QM, QI, 0, LR, NR, NP / E / xs]TAB1 (xstable)
SEND
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse MF3: {:?}", result.err());
}

#[test]
fn parse_mf2_mt151_partial() {
    // A significant portion of MF2 MT151 with nested structures
    let input = r#"
[MAT, 2,151/ ZA, AWR, 0, 0, NIS, 0]HEAD

for i=1 to NIS:
(isotope[i])
    [MAT, 2,151/ ZAI, ABN, 0, LFW, NER, 0]CONT
    for j=1 to NER:
    (range[j])
        [MAT, 2,151/ EL, EH, LRU, LRF, NRO, NAPS]CONT

        # Special case
        if LRU==0 and LRF==0 and NRO==0 and NAPS==0 and LFW==0 and NER==1:
            [MAT, 2,151/ SPI, AP, 0, 0, 0, 0]CONT

        # Resolved resonance data
        elif LRU==1:

            # SLBW or MLBW
            if LRF==1 or LRF==2:
                if NRO != 0:
                    [MAT, 2,151/ 0.0, 0.0, 0, 0, NR, NP/ Eint / AP]TAB1 (AP_table)
                endif

                if NRO!=0 and (NAPS==0 or NAPS==1):
                    [MAT, 2,151/ SPI, 0.0, 0, 0, NLS, 0]CONT
                else:
                    [MAT, 2,151/ SPI, AP, 0, 0, NLS, 0]CONT
                endif

                for m=1 to NLS:
                (l_group[m])
                    [MAT, 2,151/ AWRI, QX, L, LRX, 6*NRS, NRS /
                    {ER[k], AJ[k], GT[k], GN[k], GG[k], GF[k]}{k=1 to NRS} ]LIST
                (/l_group[m])
                endfor

            # R-matrix Limited (RML)
            elif LRF==7:
                [MAT,2,151/ 0.0, 0.0, IFG, KRM, NJS, KRL ]CONT
                [MAT,2,151/0.0, 0.0, NPP, 0, 12*NPP, 2*NPP /
                    {MA[k] , MB[k], ZA[k] , ZB[k] , IA[k] , IB[k] ,
                    Q[k], PNT [k], SHF[k] , MT[k] , PA[k] , PB[k]}{k=1 to NPP} ]LIST
                for k=1 to NJS:
                (j_group[k])
                    [MAT,2,151/ AJ, PJ, KBK, KPS, 6*NCH, NCH /
                    {PPI[l] , L[l] , SCH[l] , BND [l] , APE[l] , APT[l]}{l=1 to NCH} ]LIST

                    if NRS > 0 [lookahead=1]:
                        NX := (1+NCH + (5-NCH) % 6) * NRS / 6
                        num_zeros := (5-NCH) % 6
                        [MAT,2,151/ 0.0, 0.0, 0, NRS, 6*NX, NX /
                            { ER[n], {GAM[m,n]}{m=1 to NCH},
                              {0.0}{p=1 to num_zeros} }{n=1 to NRS} ]LIST

                    # no resonances in the spin group
                    elif NRS==0 and NX==1 [lookahead=1]:
                        [MAT,2,151/ 0.0, 0.0, 0, NRS, 6*NX, NX /
                            {0.0}{m=1 to 6}]LIST
                    endif

                (/j_group[k])
                endfor
            endif

        endif
    (/range[j])
    endfor
(/isotope[i])
endfor
SEND
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse MF2 MT151 partial: {:?}", result.err());
}

#[test]
fn parse_stop_with_message() {
    let input = "\nstop(\"Format error: NX<1 or NX>3 for LRF=4 (Adler-Adler)\")\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse stop with message: {:?}", result.err());
}

#[test]
fn parse_mf32_division_patterns() {
    // Test the complex division pattern from MF32
    let input = r#"
[MAT,32,151/ 0.0, 0.0, MPAR, 0, 6*NRB+(MPAR*NRB)*(MPAR*NRB+1)/2, NRB/
{ER[k], AJ[k], GT[k], GN[k], GG[k], GF[k]}{k=1 to NRB},
{{V[m,n]}{n=m to MPAR*NRB}}{m=1 to MPAR*NRB} ] LIST
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse MF32 division pattern: {:?}", result.err());
}

#[test]
fn parse_mf32_parenthesized_6th_field() {
    // 6th field with division inside parentheses
    let input = r#"
[MAT,32,151/ 0.0, 0.0, 0, 0, JCH, (1+(NCH-1)/6) /
{{DAP[m,n]}{n=1 to NCH}}{m=1 to JCH/NCH} ] LIST
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse parenthesized 6th field: {:?}", result.err());
}

#[test]
fn parse_errorr_mf33_repeat() {
    let input = r#"
[MAT, 33, MT/ ZA, AWR, 0, MTL, 0, NK] CONT
for k=1 to NK:
    (subsection[k])
        [MAT, 33, MT/ 0.0, 0.0, MAT1, MT1, 0, NG] CONT
        repeat [i=1]:
          [MAT, 33, MT/ 0.0, 0.0, NG1[i], IG1[i], NG1[i], IG[i] /
              {COV[i,j]}{j=1 to NG1[i]}] LIST
        until IG[i] == NG
    (/subsection[k])
endfor
SEND
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse ERRORR MF33 repeat: {:?}", result.err());
}

#[test]
fn parse_mf8_mt457_partial() {
    let input = r#"
# radiactive nucleus
if NST==0 [lookahead=1]:
    [MAT, 8,457/ ZA, AWR, LIS, LISO, NST, NSP]HEAD
    [MAT, 8,457/ Thalf , dThalf , 0, 0, 2*NC, 0 /
        {Ebar_x[k] ,dEbar_x[k]}{k=1 to NC} ]LIST
    [MAT, 8,457/ SPI, PAR, 0, 0, 6*NDK, NDK/
        {RTYP[k] , RFS[k] , Q[k] , dQ[k] , BR[k] , dBR[k]}{k=1 to NDK} ]LIST
    for k=1 to NSP:
    (spectrum[k])
        [MAT, 8,457/ 0.0, STYP, LCON, LCOV, 6, NER/
            FD, dFD, ERAV , dERAV , FC, dFC] LIST

        if LCON != 1:
            (discrete)
              for i=1 to NER:
                (energysec[i])
                  if NT == 6 [lookahead=1]:
                      [MAT, 8,457/ ER , dER, 0, 0, NT, 0/
                      RTYP , TYPE , RI , dRI,   RIS , dRIS ]LIST
                  elif NT == 8 [lookahead=1]:
                      [MAT, 8,457/ ER , dER, 0, 0, NT, 0/
                      RTYP , TYPE , RI , dRI,   RIS , dRIS,
                      RICC, dRICC ]LIST
                  elif NT == 12 [lookahead=1]:
                      [MAT, 8,457/ ER , dER, 0, 0, NT, 0/
                      RTYP , TYPE , RI , dRI,   RIS , dRIS ,
                      RICC ,dRICC , RICK,dRICK, RICL ,dRICL ] LIST
                  endif
                (/energysec[i])
              endfor
            (/discrete)
        endif
        if LCON != 0:
            (continuous)
                [MAT, 8,457/ RTYP, 0.0, 0, 0, NR, NP/ Eint / RP ] TAB1
            (/continuous)
        endif
    (/spectrum[k])
    endfor

# stable nucleus
elif NST==1 [lookahead=1]:
    [MAT, 8,457/ ZA, AWR, LIS, LISO, NST, 0]HEAD
    [MAT, 8,457/ 0.0, 0.0, 0, 0, 6, 0/
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0] LIST
    [MAT, 8,457/ SPI, PAR, 0, 0, 6, 0 /
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0] LIST
endif
SEND
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse MF8 MT457: {:?}", result.err());
}

#[test]
fn parse_intg_record() {
    let input = "\n[MAT, 32,151/ II, JJ, KIJ {NDIGIT}]INTG\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse INTG record: {:?}", result.err());
}

#[test]
fn parse_list_with_nested_loops_and_division() {
    // From MF8 MT457 covariance section
    let input = r#"
[MAT, 8,457/ 0.0, 0.0, LS, 5, NE, NERP/
{E[m]}{m=1 to NERP}, {{F[m,n]}{n=m to NERP-2}}{m=1 to NERP-2} ] LIST
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse list with nested loops: {:?}", result.err());
}

#[test]
fn parse_mf40_division_in_list_for_stop() {
    // Division in list_for_head for_stop: (NT-1)/NER-1
    let input = r#"
[MAT, 40, MT/ 0.0, 0.0, LS, LB, NT, NER/
{E[q]}{q=1 to NER} {{F[q,l]}{l=1 to (NT-1)/NER-1}}{q=1 to NER-1} ]LIST
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse MF40 division in list for stop: {:?}", result.err());
}

#[test]
fn parse_inconsistent_varspec_in_record() {
    // NX? is an inconsistent_varspec
    let input = r#"
[MAT,32,151/0.0, 0.0, 0, NRSA, 12*NX, NX?/
{ ER[k], {GAM[p,k]}{p=1 to NCH}, {0.0}{r=1 to num_zeros}
  DER[k], {DGAM[p,k]}{p=1 to NCH}, {0.0}{r=1 to num_zeros}
}{k=1 to NRSA} ]LIST
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse inconsistent_varspec: {:?}", result.err());
}

#[test]
fn parse_for_loop_with_division_in_stop() {
    // Division in for_stop
    let input = "\nfor n=1 to NL*(NL+1)/2:\n[MAT, 34, MT/ SPI, AP, 0, 0, NLS, 0]CONT\nendfor\n";
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse for with division in stop: {:?}", result.err());
}

#[test]
fn parse_full_mf2_mt151() {
    let input = r#"
[MAT, 2,151/ ZA, AWR, 0, 0, NIS, 0]HEAD

for i=1 to NIS:
(isotope[i])
    [MAT, 2,151/ ZAI, ABN, 0, LFW, NER, 0]CONT
    for j=1 to NER:
    (range[j])
        [MAT, 2,151/ EL, EH, LRU, LRF, NRO, NAPS]CONT

        # Special case for a single isotope without
        # resonance parameters and scattering radius only given
        if LRU==0 and LRF==0 and NRO==0 and NAPS==0 and LFW==0 and NER==1:
            [MAT, 2,151/ SPI, AP, 0, 0, 0, 0]CONT

        # Resolved resonance data
        elif LRU==1:

            # Single level Breit-Wigner (SLBW) or Multi level Breit-Wigner (MLBW)
            if LRF==1 or LRF==2:
                if NRO != 0:
                    [MAT, 2,151/ 0.0, 0.0, 0, 0, NR, NP/ Eint / AP]TAB1 (AP_table)
                endif

                if NRO!=0 and (NAPS==0 or NAPS==1):
                    [MAT, 2,151/ SPI, 0.0, 0, 0, NLS, 0]CONT
                else:
                    [MAT, 2,151/ SPI, AP, 0, 0, NLS, 0]CONT
                endif

                for m=1 to NLS:
                (l_group[m])
                    [MAT, 2,151/ AWRI, QX, L, LRX, 6*NRS, NRS /
                    {ER[k], AJ[k], GT[k], GN[k], GG[k], GF[k]}{k=1 to NRS} ]LIST
                (/l_group[m])
                endfor

            # R-matrix Reich-Moore multi level parameters
            elif LRF==3:
                if NRO != 0:
                    [MAT, 2,151/ 0.0, 0.0, 0, 0, NR, NP/ Eint / AP]TAB1 (AP_table)
                endif

                if NRO!=0 and (NAPS==0 or NAPS==1):
                    [MAT, 2,151/ SPI, 0.0, LAD, 0, NLS, NLSC]CONT
                else:
                    [MAT, 2,151/ SPI, AP, LAD, 0, NLS, NLSC]CONT
                endif

                for m=1 to NLS:
                (l_group[m])
                    [MAT, 2,151/AWRI,APL, L, 0, 6*NRS, NRS/
                    {ER[k] , AJ[k] , GN[k], GG[k] , GFA[k],  GFB[k]}{k=1 to NRS} ]LIST
                (/l_group[m])
                endfor

            # Adler-Adler formalism
            elif LRF==4:
                if NRO != 0:
                    [MAT, 2,151/ 0.0, 0.0, 0, 0, NR, NP/ Eint / AP]TAB1 (AP_table)
                endif

                if NRO!=0 and (NAPS==0 or NAPS==1):
                    [MAT, 2,151/ SPI, 0.0, 0, 0, NLS, 0]CONT
                else:
                    [MAT, 2,151/ SPI, AP, 0, 0, NLS, 0]CONT
                endif
                #
                # LI: Flag to indicate the kind of parameters given
                #
                if NX==1 [lookahead=1]:
                    [MAT, 2,151/ AWRI, 0.0, LI, 0, 6*NX, NX /
                        {AT[k]}{k=1 to 4}, {BT[k]}{k=1 to 2}]LIST
                elif NX==2 [lookahead=1]:
                    [MAT, 2,151/ AWRI, 0.0, LI, 0, 6*NX, NX /
                        {AT[k]}{k=1 to 4}, {BT[k]}{k=1 to 2},
                        {AC[k]}{k=1 to 4}, {BC[k]}{k=1 to 2}]LIST
                elif NX==3 [lookahead=1]:
                    [MAT, 2,151/ AWRI, 0.0, LI, 0, 6*NX, NX /
                        {AT[k]}{k=1 to 4}, {BT[k]}{k=1 to 2},
                        {AF[k]}{k=1 to 4}, {BF[k]}{k=1 to 2},
                        {AC[k]}{k=1 to 4}, {BC[k]}{k=1 to 2}]LIST
                else:
                    stop("Format error: NX<1 or NX>3 for LRF=4 (Adler-Adler)")
                endif
                for m=1 to NLS:
                (l_group[m])
                    [MAT, 2,151/0.0, 0.0, L, 0, NJS, 0] CONT
                    for n=1 to NJS:
                    (j_group[n])
                        [MAT, 2,151/ AJ, 0.0, 0, 0, 12*NLJ, NLJ/
                            {DET[k],DWT[k],GRT[k],GIT[k],DEF[k],DWF[k],
                            GRF[k],GIF[k],DEC[k],DWC[k],GRC[k],GIC[k]}{k=1 to NLJ}] LIST
                    (/j_group[n])
                    endfor
                (/l_group[m])
                endfor

            # R-matrix Limited (RML)
            elif LRF==7:
                [MAT,2,151/ 0.0, 0.0, IFG, KRM, NJS, KRL ]CONT
                [MAT,2,151/0.0, 0.0, NPP, 0, 12*NPP, 2*NPP /
                    {MA[k] , MB[k], ZA[k] , ZB[k] , IA[k] , IB[k] ,
                    Q[k], PNT [k], SHF[k] , MT[k] , PA[k] , PB[k]}{k=1 to NPP} ]LIST
                for k=1 to NJS:
                (j_group[k])
                    [MAT,2,151/ AJ, PJ, KBK, KPS, 6*NCH, NCH /
                    {PPI[l] , L[l] , SCH[l] , BND [l] , APE[l] , APT[l]}{l=1 to NCH} ]LIST

                    if NRS > 0 [lookahead=1]:
                        NX := (1+NCH + (5-NCH) % 6) * NRS / 6
                        num_zeros := (5-NCH) % 6
                        [MAT,2,151/ 0.0, 0.0, 0, NRS, 6*NX, NX /
                            { ER[n], {GAM[m,n]}{m=1 to NCH},
                              {0.0}{p=1 to num_zeros} }{n=1 to NRS} ]LIST

                    # no resonances in the spin group
                    elif NRS==0 and NX==1 [lookahead=1]:
                        [MAT,2,151/ 0.0, 0.0, 0, NRS, 6*NX, NX /
                            {0.0}{m=1 to 6}]LIST
                    endif

                    if KBK > 0:
                        for n=1 to KBK:
                            [MAT,2,151/ 0.0, 0.0, LCH, LBK, 0, 0 ]CONT
                            if LBK == 1:
                                [MAT,2,151/ 0.0, 0.0, LCH, LBK, 0, 0 ]CONT
                                [MAT,2,151/ 0.0, 0.0, 0, 0, NR, NP/ E / RBR]TAB1 (real_part[n])
                                [MAT,2,151/ 0.0, 0.0, 0, 0, NR, NP/ E / RBI]TAB1 (imag_part[n])
                            elif LBK == 2:
                                [MAT,2,151/ ED, EU, 0, 0, 5, 0/ R0, R1, R2, S0, S1 ]LIST
                            elif LBK == 3:
                                [MAT,2,151/ ED, EU, 0, 0, 3, 0/ R0, S0, GA ]LIST
                            endif
                        endfor
                    endif
                    if KPS > 0:
                        for n=1 to NCH:
                            [MAT,2,151/ 0.0, 0.0, 0, 0, LPS, 1/
                                0.0, 0.0, 0.0, 0.0, 0.0, 0.0]LIST
                            if LPS == 1:
                                [MAT,2,151/ 0.0, 0.0, 0, 0, NR, NP/ E / PSR ]TAB1 (real_part[n])
                                [MAT,2,151/ 0.0, 0.0, 0, 0, NR, NP/ E / PSI ]TAB1 (imag_part[n])
                            endif
                        endfor
                    endif

                (/j_group[k])
                endfor
            endif

        # Unresolved resonance data
        elif LRU==2:

            # Case A
            if LFW==0 and LRF==1:
                if NRO != 0:
                    [MAT, 2,151/ 0.0, 0.0, 0, 0, NR, NP/ Eint /AP]TAB1 (AP_table)
                endif

                if NRO!=0 and (NAPS==0 or NAPS==1):
                    [MAT, 2,151/ SPI, 0.0, LSSF, 0, NLS, 0] CONT
                else:
                    [MAT, 2,151/ SPI, AP, LSSF, 0, NLS, 0] CONT
                endif

                for p=1 to NLS:
                (l_group[p])
                    [MAT, 2,151/AWRI, 0.0, L, 0, 6*NJS, NJS/
                    {D[m], AJ[m], AMUN[m], GN0[m], GG[m], 0.0}{m=1 to NJS}] LIST
                (/l_group[p])
                endfor

            # Case B
            elif LFW==1 and LRF==1:
                if NRO != 0:
                    [MAT, 2,151/ 0.0, 0.0, 0, 0, NR, NP/ Eint /AP]TAB1 (AP_table)
                endif

                if NRO!=0 and (NAPS==0 or NAPS==1):
                    [MAT, 2,151/ SPI, 0.0, LSSF, 0, NE, NLS /
                                {ES[p]}{p=1 to NE} ] LIST
                else:
                    [MAT, 2,151/ SPI, AP, LSSF, 0, NE, NLS /
                                {ES[p]}{p=1 to NE} ] LIST
                endif

                for p=1 to NLS:
                (l_group[p])
                    [MAT, 2,151/ AWRI, 0.0, L, 0, NJS, 0]CONT
                    for n=1 to NJS:
                    (j_group[n])
                        [MAT, 2,151/ 0.0, 0.0, L, MUF, NE+6, 0/
                            D, AJ, AMUN, GN0, GG, 0.0,
                            {GF[m]}{m=1 to NE} ] LIST
                    (/j_group[n])
                    endfor
                (/l_group[p])
                endfor

            # Case C
            elif (LFW==0 or LFW==1) and LRF==2:
                if NRO != 0:
                    [MAT, 2,151/ 0.0, 0.0, 0, 0, NR, NP/ Eint /AP] TAB1 (AP_table)
                endif

                if NRO!=0 and (NAPS==0 or NAPS==1):
                    [MAT, 2,151/ SPI, 0.0, LSSF, 0, NLS, 0]CONT
                else:
                    [MAT, 2,151/ SPI, AP, LSSF, 0, NLS, 0]CONT
                endif

                for p=1 to NLS:
                (l_group[p])
                    [MAT, 2,151/ AWRI, 0.0, L, 0, NJS, 0]CONT
                    for n=1 to NJS:
                    (j_group[n])
                        [MAT, 2,151/ AJ, 0.0, INT, 0, 6*NE+6, NE/
                            0.0, 0.0, AMUX, AMUN, AMUG, AMUF,
                            {ES[m], D[m] , GX[m] , GN0[m] , GG[m] , GF[m]}{m=1 to NE} ]LIST
                    (/j_group[n])
                    endfor
                (/l_group[p])
                endfor
            endif
        endif
    (/range[j])
    endfor
(/isotope[i])
endfor
SEND
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse full MF2 MT151: {:?}", result.err());
}

#[test]
fn parse_full_mf32() {
    let input = r#"
[MAT,32,151/ ZA, AWR, 0, 0, NIS, 0]HEAD
for i=1 to NIS:
(isotope[i])
    [MAT,32,151/ ZAI, ABN, 0, LFW, NER, 0]CONT
    for j=1 to NER:
    (range[j])
        [MAT,32,151/ EL, EH, LRU, LRF, NRO, NAPS]CONT
        #
        # Energy-dependent covariance data for the scattering radius AP(E)
        #
        if NRO != 0:
            [MAT,32,151/ 0.0, 0.0, 0, 0, 0, NI]CONT
            for m=1 to NI:
            (AP_subsec[m])
                if LB>=0 and LB<=4 [lookahead=1]:
                    [MAT,32,151/ 0.0, 0.0, LT, LB, 2*NP, NP/
                        {Ek[k] , Fk[k]}{k=1 to (NP-LT)}
                        {El[k] , Fl[k]}{k=1 to LT} ]LIST
                elif LB==5 and LS==0 [lookahead=1]:
                    # asymmetric matrix
                    NT := NE*(NE-1)+1
                    [MAT,32,151/ 0.0, 0.0, LS, LB, NT, NE/
                        {E[k]}{k=1 to NE} {{F[k,kp]}{kp=1 to NE-1}}{k=1 to NE-1} ]LIST
                elif LB==5 and LS==1 [lookahead=1]:
                    # symmetric matrix
                    NT := NE*(NE+1)/2
                    [MAT,32,151/ 0.0, 0.0, LS, LB, NT, NE/
                        {E[k]}{k=1 to NE} {{F[k,kp]}{kp=k to NE-1}}{k=1 to NE-1} ]LIST
                else:
                    stop("LB<0 and LB>5 not implemented for the scattering radius covariance")
                endif
            (/AP_subsec[m])
            endfor
        endif

        if LCOMP==0 and LRU==1 and (LRF==1 or LRF==2) [lookahead=1]:
            [MAT,32,151/ SPI, AP, 0, LCOMP, NLS, ISR]CONT
            if ISR != 0:
                [MAT,32,151/ 0.0, DAP, 0, 0, 0, 0]CONT
            endif
            for k=1 to NLS:
                (l_group[k])
                    [MAT,32,151/AWRI, 0.0, L, 0, 18*NRS, NRS /
                        {ER[m],  AJ[m],   GT[m],   GN[m],    GG[m],  GF[m],
                         DE2[m], DN2[m],  DNDG[m], DG2[m],  DNDF[m], DGDF[m],
                         DF2[m], DJDN[m], DJDG[m], DJDF[m], DJ2[m],  0.0}{m=1 to NRS} ]LIST
                (/l_group[k])
            endfor

        elif LCOMP==1 and LRU==1 and LRF != 7 [lookahead=1]:
            [MAT,32,151/ SPI, AP, 0, LCOMP, NLS, ISR]CONT

            if LRF==1 or LRF==2:
                if ISR > 0:
                    [MAT,32,151/ 0.0, DAP, 0, 0, 0, 0]CONT
                endif
                [MAT,32,151/ AWRI, 0.0, 0, 0, NSRS, NLRS]CONT
                for p=1 to NSRS:
                    (sr_subsec[p])
                        [MAT,32,151/ 0.0, 0.0, MPAR, 0, 6*NRB+(MPAR*NRB)*(MPAR*NRB+1)/2, NRB/
                            {ER[k], AJ[k], GT[k], GN[k], GG[k], GF[k]}{k=1 to NRB},
                            {{V[m,n]}{n=m to MPAR*NRB}}{m=1 to MPAR*NRB} ] LIST
                    (/sr_subsec[p])
                endfor

            elif LRF==3:
                if ISR > 0:
                    [MAT,32,151/ 0.0, 0.0, 0, 0, MLS, 1 /
                        {DAP[k]}{k=1 to MLS} ]LIST
                endif
                [MAT,32,151/ AWRI, 0.0, 0, 0, NSRS, NLRS]CONT
                for p=1 to NSRS:
                    (sr_subsec[p])
                        [MAT,32,151/ 0.0, 0.0, MPAR, 0, 6*NRB+(MPAR*NRB)*(MPAR*NRB+1)/2, NRB/
                            {ER[k], AJ[k], GN[k], GG[k], GFA[k], GFB[k]}{k=1 to NRB},
                            {{V[m,n]}{n=m to MPAR*NRB}}{m=1 to MPAR*NRB} ] LIST
                    (/sr_subsec[p])
                endfor

            elif LRF==4:
                if ISR > 0:
                    [MAT,32,151/ 0.0, DAP, 0, 0, 0, 0]CONT
                endif
                [MAT,32,151/ AWRI, 0.0, 0, 0, NSRS, NLRS]CONT
                for p=1 to NSRS:
                    (sr_subsec[p])
                        [MAT,32,151/ 0.0, 0.0, MPAR, 0, 12*NRB+(MPAR*NRB)*(MPAR*NRB+1)/2, NRB/
                            {DET[k],DWT[k],GRT[k],GIT[k],DEF[k],DWF[k],
                             GRF[k],GIF[k],DEC[k],DWC[k],GRC[k],GIC[k]}{k=1 to NRB},
                            {{V[m,n]}{n=m to MPAR*NRB}}{m=1 to MPAR*NRB} ] LIST
                    (/sr_subsec[p])
                endfor

            endif

            # Long range components
            if NLRS > 0:
                for p=1 to NLRS:
                (lr_subsec[p])
                    if LB >= -1 and LB <= 2 [lookahead=1]:
                        [MAT,32,151/ 0.0, 0.0, IDP, LB, 2*NEB, NEB/
                             {Ek[k] , Fk[k]}{k=1 to NEB}] LIST
                    elif LB==5 [lookahead=1]:
                        [MAT,32,151/ 0.0, 0.0, IDP, LB, NEB*(NEB+1)/2, NEB/
                             {E[k]}{k=1 to NEB} {{F[k,kp]}{kp=k to NEB-1}}{k=1 to NEB-1} ]LIST
                    else:
                        stop("LB < -1, LB = 3,4 and LB > 5 not allowed for long-range components")
                    endif
                (/lr_subsec[p])
                endfor
            endif

        elif LCOMP==1 and LRU==1 and LRF==7 [lookahead=1]:
            # R-Matrix Limited formalism
            [MAT,32,151/ 0.0, 0.0, 0, LCOMP, 0, ISR]CONT
            if ISR > 0:
                [MAT,32,151/ 0.0, 0.0, 0, 0, JCH, (1+(NCH-1)/6) /
                    {{DAP[m,n]}{n=1 to NCH}}{m=1 to JCH/NCH} ] LIST
            endif
            [MAT,32,151/ AWRI, 0.0, 0, 0, NSRS, 0]CONT
            for k=1 to NSRS:
                (sr_subsec[k])
                    [MAT,32,151/ 0.0, 0.0, NJSX, 0, 0, 0]CONT
                    for m=1 to NJSX:
                        (j_group[m])
                            NX := (1+NCH + (5-NCH) % 6) * NRS / 6
                            num_zeros := (5-NCH) % 6
                            [MAT,32,151/ 0.0, 0.0, NCH, NRB, 6*NX, NX/
                                { ER[p], {GAM[q,p]}{q=1 to NCH}
                                  {0.0}{r=1 to num_zeros} }{p=1 to NRB} ]LIST
                        (/j_group[m])
                    endfor
                    N := (NPARB*(NPARB+1))/2
                    [MAT,32,151/ 0.0, 0.0, 0, 0, N, NPARB/
                       {{V[p,q]}{q=p to NPARB}}{p=1 to NPARB} ] LIST
                (/sr_subsec[k])
            endfor

        elif LCOMP==2 and LRU==1 and (LRF==1 or LRF==2) [lookahead=1]:
            [MAT,32,151/ SPI, AP, 0, LCOMP, 0, ISR]CONT
            if ISR > 0:
                [MAT,32,151/ 0.0, DAP, 0, 0, 0, 0]CONT
            endif
            [MAT,32,151/AWRI, QX, 0, LRX, 12*NRSA, NRSA/
                {ER[k],  AJ[k], GT[k], GN[k],  GG[k],  GF[k],
                 DER[k], 0.0,   0.0,   DGN[k], DGG[k], DGF[k]}{k=1 to NRSA} ]LIST
            [MAT,32,151/ 0.0, 0.0, NDIGIT, NNN, NM, 0 ]CONT
            for k=1 to NM:
                [MAT,32,151/ II[k], JJ[k], KIJ[k] {NDIGIT}]INTG
            endfor

        elif LCOMP==2 and LRU==1 and LRF==3 [lookahead=1]:
            [MAT,32,151/ SPI, AP, LAD, LCOMP, 0, ISR]CONT
            if ISR > 0:
                [MAT,32,151/ 0.0, 0.0, 0, 0, MLS, 1 /
                    {DAP[k]}{k=1 to MLS} ]LIST
            endif
            [MAT,32,151/AWRI, APL, 0, 0, 12*NRSA, NRSA/
                {ER[k],  AJ[k], GN[k],  GG[k],  GFA[k],  GFB[k],
                 DER[k], 0.0,   DGN[k], DGG[k], DGFA[k], DGFB[k]}{k=1 to NRSA} ]LIST
             [MAT,32,151/ 0.0, 0.0, NDIGIT, NNN, NM, 0 ]CONT
             for k=1 to NM:
                [MAT,32,151/ II[k], JJ[k], KIJ[k]{NDIGIT}]INTG
             endfor

        elif LCOMP==2 and LRU==1 and LRF==7 [lookahead=1]:
            [MAT,32,151/ 0.0, 0.0, IFG, LCOMP, NJS, ISR ]CONT
            if ISR > 0:
                [MAT,32,151/ 0.0, 0.0, 0, 0, NJCH, (1+(NJCH-1)/6) /
                    {{DAP[m,n]}{n=1 to NJCH/NJS}}{m=1 to NJS} ] LIST
            endif
            [MAT,32,151/ 0.0, 0.0, NPP, NJSX, 12*NPP, 2*NPP/
                {MA[k], MB[k],  ZA[k],  ZB[k], IA[k], IB[k],
                 Q[k],  PNT[k], SHF[k], MT[k], PA[k], PB[k]}{k=1 to NPP} ]LIST
            for q=1 to NJS:
                (j_group[q])
                    [MAT,32,151/ AJ, PJ, 0, 0, 6*NCH, NCH/
                        {PPI[k], L[k], SCH[k], BND[k], APE[k], APT[k]}{k=1 to NCH} ]LIST
                    NX := (2*(NCH+1) + 2*((5-NCH) % 6)) * NRSA / 12
                    num_zeros := (5-NCH) % 6
                    [MAT,32,151/0.0, 0.0, 0, NRSA, 12*NX, NX?/
                        { ER[k], {GAM[p,k]}{p=1 to NCH}, {0.0}{r=1 to num_zeros}
                          DER[k], {DGAM[p,k]}{p=1 to NCH}, {0.0}{r=1 to num_zeros}
                        }{k=1 to NRSA} ]LIST
                (/j_group[q])
            endfor
            [MAT,32,151/ 0.0, 0.0, NDIGIT, NNN, NM, 0 ]CONT
            for q=1 to NM:
                [MAT,32,151/ II[q], JJ[q], KIJ[q]{NDIGIT} ]INTG
            endfor

        elif LRU == 2:
            [MAT,32,151/ SPI, AP, 0, 0, NLS, 0]CONT
            for q=1 to NLS:
                (l_group[q])
                    [MAT,32,151/ AWRI, 0.0, L, 0, 6*NJS, NJS/
                        {D[k], AJ[k], GN0[k], GG[k], GF[k], GX[k]}{k=1 to NJS}]LIST
                (/l_group[q])
            endfor
            [MAT,32,151/ 0.0, 0.0, MPAR, 0, (NPAR*(NPAR+1))/2, NPAR/
                {{RV[p,q]}{q=p to NPAR}}{p=1 to NPAR} ]LIST
        endif

    (/range[j])
    endfor
(/isotope[i])
endfor
SEND
"#;
    let result = RecipeParser::parse(Rule::endf_recipe, input);
    assert!(result.is_ok(), "Failed to parse full MF32: {:?}", result.err());
}
