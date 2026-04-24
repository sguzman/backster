(* Rolling p-value distro fit (legacy) + JSON wrappers for WSTP.

   The core rolling-fit logic below is copied from the original "math1" script
   so results match Wolfram's EstimatedDistribution + DistributionFitTest.

   Rust calls:
     BacksterFinancialDataCloseAndFitJSON["AAPL","2020-01-01","2021-01-01","Close",126]
     BacksterExprCloseAndFitJSON["TimeSeries[...]", 126]
*)

ClearAll[
  getReturnSeriesFromData,
  makeRollingWindows,
  safeEstimatedDistribution,
  safePValue,
  rollingDistributionPValues
];

getReturnSeriesFromData[data_] :=
  Module[{dates, prices, rets},
    dates = data["Dates"];
    prices = N@QuantityMagnitude@data["Values"];
    rets = N@Differences[Log[prices]];
    <|"Dates" -> Rest[dates], "Returns" -> rets|>
  ];

makeRollingWindows[dates_List, vals_List, window_Integer?Positive] :=
  Module[{n = Length[vals]},
    Table[
      <|"EndDate" -> dates[[i + window - 1]], "Data" -> vals[[i ;; i + window - 1]]|>,
      {i, 1, n - window + 1}
    ]
  ];

safeEstimatedDistribution[data_, family_] :=
  Quiet@Check[EstimatedDistribution[data, family], $Failed];

safePValue[data_, fit_] :=
  Quiet@Check[DistributionFitTest[data, fit, "PValue"], Missing["NotAvailable"]];

rollingDistributionPValues[data_, distFamilies_List, windowSize_ : 126] :=
  Module[{parsed, dates, rets, windows, familyNames, scores},
    parsed = getReturnSeriesFromData[data];
    dates = parsed["Dates"];
    rets = parsed["Returns"];
    If[windowSize > Length[rets], Return[$Failed]];
    windows = makeRollingWindows[dates, rets, windowSize];
    familyNames = ToString[InputForm[#]] & /@ distFamilies;
    scores =
      AssociationThread[
        familyNames,
        Table[
          Module[{fam = distFamilies[[j]]},
            Table[
              Module[{wdata, fit, p},
                wdata = windows[[i, "Data"]];
                fit = safeEstimatedDistribution[wdata, fam];
                p = If[fit === $Failed, Missing["FitFailed"], safePValue[wdata, fit]];
                {windows[[i, "EndDate"]], p}
              ],
              {i, Length[windows]}
            ]
          ],
          {j, Length[distFamilies]}
        ]
      ];
    <|"WindowSize" -> windowSize, "Scores" -> scores|>
  ];

(* ---- WSTP-facing wrappers ---- *)

ClearAll[
  BacksterFinancialDataCloseAndFitJSON,
  BacksterExprCloseAndFitJSON,
  backsterCloseRows,
  backsterRollingFitRows,
  backsterMaybeNumber
];

backsterMaybeNumber[x_] := If[NumericQ[x], N[x], Null];

backsterCloseRows[data_] :=
  Module[{dates, values},
    dates = data["Dates"];
    values = N@QuantityMagnitude@data["Values"];
    Transpose[{1000*UnixTime /@ dates, values}]
  ];

backsterRollingFitRows[data_, distFamilies_List, windowSize_Integer?Positive] :=
  Module[{parsed, dates, rets, windows, safeVal, rows},
    parsed = getReturnSeriesFromData[data];
    dates = parsed["Dates"];
    rets = parsed["Returns"];
    If[windowSize > Length[rets], Return[{}]];
    windows = makeRollingWindows[dates, rets, windowSize];

    (* Return a single random draw from the fitted distribution.
       This matches the "yield a random value from each distro" requirement. *)
    safeVal[fit_] := Quiet@Check[First@RandomVariate[fit, 1], Missing["NotAvailable"]];

    rows =
      Table[
        Module[{wdata, end, fits, ps0, vs0, ps, vs},
          wdata = windows[[i, "Data"]];
          end = windows[[i, "EndDate"]];
          fits = safeEstimatedDistribution[wdata, #] & /@ distFamilies;

          ps0 = If[# === $Failed, Missing["FitFailed"], safePValue[wdata, #]] & /@ fits;
          vs0 = If[# === $Failed, Missing["FitFailed"], safeVal[#]] & /@ fits;

          ps = backsterMaybeNumber /@ ps0;
          vs = backsterMaybeNumber /@ vs0;

          Flatten[{1000*UnixTime[end], Riffle[ps, vs]}]
        ],
        {i, Length[windows]}
      ];

    rows
  ];

distFamilies = {
  NormalDistribution[\[Mu], \[Sigma]],
  StudentTDistribution[\[Nu], \[Mu], \[Sigma]],
  LaplaceDistribution[\[Mu], \[Sigma]],
  LogisticDistribution[\[Mu], \[Beta]],
  CauchyDistribution[\[Alpha], \[Beta]]
};

BacksterFinancialDataCloseAndFitJSON[
  symbol_String,
  start_String,
  end_String,
  field_String : "Close",
  windowSize_Integer : 126
] :=
  Module[{ts, closeRows, fitRows},
    ts =
      TimeConstrained[
        Quiet@Check[
          FinancialData[
            symbol,
            {DateObject[start], DateObject[end]},
            field
          ],
          $Failed
        ],
        20,
        $Failed
      ];
    If[ts === $Failed, Return[ExportString[<|"close" -> {}, "fit" -> {}|>, "JSON"]]];

    closeRows = backsterCloseRows[ts];
    If[windowSize <= 0, Return[ExportString[<|"close" -> closeRows, "fit" -> {}|>, "JSON"]]];
    fitRows = backsterRollingFitRows[ts, distFamilies, windowSize];

    ExportString[<|"close" -> closeRows, "fit" -> fitRows|>, "JSON"]
  ];

BacksterExprCloseAndFitJSON[
  expr_String,
  windowSize_Integer : 126
] :=
  Module[{ts, closeRows, fitRows},
    ts =
      TimeConstrained[
        Quiet@Check[ToExpression[expr], $Failed],
        20,
        $Failed
      ];
    If[ts === $Failed, Return[ExportString[<|"close" -> {}, "fit" -> {}|>, "JSON"]]];

    closeRows = backsterCloseRows[ts];
    If[windowSize <= 0, Return[ExportString[<|"close" -> closeRows, "fit" -> {}|>, "JSON"]]];
    fitRows = backsterRollingFitRows[ts, distFamilies, windowSize];

    ExportString[<|"close" -> closeRows, "fit" -> fitRows|>, "JSON"]
  ];
